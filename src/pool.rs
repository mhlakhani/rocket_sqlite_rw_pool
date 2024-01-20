use crate::{
    config::Config, holder::ConnectionHolder, migration::run_migrations, util::run_blocking,
    Connector, Error, ReadConnection, WriteAuthorization, WriteConnection,
};

use std::{marker::PhantomData, sync::Arc, time::Duration};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rocket::{
    fairing::{AdHoc, Fairing},
    Build, Phase, Rocket,
};
use rusqlite::{Connection, OpenFlags, Transaction};
use rust_embed::RustEmbed;
use tokio::{
    sync::{Mutex, Semaphore},
    time::timeout,
};

type Result<T, E = Error> = anyhow::Result<T, E>;

/// Function to run on every connection grabbed from a pool.
pub type PoolInitializerFn = fn(&Connection) -> Result<(), rusqlite::Error>;

/// Wrapper for a [`PoolInitializerFn`].
#[derive(Clone)]
pub struct PoolInitializer {
    pub initializer: PoolInitializerFn,
}

impl PoolInitializer {
    pub const fn new(initializer: PoolInitializerFn) -> Self {
        Self { initializer }
    }
}

/// Create a connection pool with the given configuration.
fn create_pool(
    config: &Config,
    is_write: bool,
    initializers: Vec<PoolInitializer>,
) -> Result<Pool<SqliteConnectionManager>> {
    let mut flags = OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if is_write {
        flags = flags | OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
    } else {
        flags |= OpenFlags::SQLITE_OPEN_READ_ONLY;
    }
    let max_size = if is_write {
        1
    } else {
        config.max_read_connections
    };
    let min_idle = if is_write {
        Some(1)
    } else {
        config.min_read_connections
    };
    let pragmas = config.pragmas.clone();
    let busy_timeout = config.busy_timeout;
    let manager = SqliteConnectionManager::file(&config.url)
        .with_flags(flags)
        .with_init(move |connection| {
            if let Some(timeout) = busy_timeout {
                connection.busy_timeout(Duration::from_secs(timeout))?;
            }
            if !is_write && !connection.is_readonly(rusqlite::DatabaseName::Main)? {
                return Err(rusqlite::Error::InvalidQuery);
            }
            pragmas.set(connection)?;
            for initializer in &initializers {
                (initializer.initializer)(connection)?;
            }
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(max_size)
        .min_idle(min_idle)
        .idle_timeout(config.idle_timeout.map(Duration::from_secs))
        .connection_timeout(Duration::from_secs(config.connect_timeout))
        .build(manager)
        .map_err(Error::PoolCreation)?;
    Ok(pool)
}

/// Pool of database connections.
pub struct ConnectionPool<DB> {
    connect_timeout: Duration,
    // This is an 'Option' so that we can drop the pool in a 'spawn_blocking'.
    writer: Option<Pool<SqliteConnectionManager>>,
    writer_semaphore: Arc<Semaphore>,
    readers: Option<Pool<SqliteConnectionManager>>,
    reader_semaphore: Arc<Semaphore>,
    _marker: PhantomData<fn() -> DB>,
}

impl<DB> Clone for ConnectionPool<DB> {
    fn clone(&self) -> Self {
        Self {
            connect_timeout: self.connect_timeout,
            writer: self.writer.clone(),
            writer_semaphore: Arc::clone(&self.writer_semaphore),
            readers: self.readers.clone(),
            reader_semaphore: Arc::clone(&self.reader_semaphore),
            _marker: PhantomData,
        }
    }
}

impl<DB: 'static> ConnectionPool<DB> {
    /// Create a new pool with the given configuration.
    fn new(config: &Config, initializers: Vec<PoolInitializer>) -> Result<Self> {
        // MUST create the writer before the reader or we get SQLITE_MISUSE (correctly!)
        let writer = Some(create_pool(config, true, initializers.clone())?);
        let readers = Some(create_pool(config, false, initializers)?);
        let writer_semaphore = Arc::new(Semaphore::new(1));
        let reader_semaphore = Arc::new(Semaphore::new(config.max_read_connections as usize));
        let connect_timeout = Duration::from_secs(config.connect_timeout);
        Ok(Self {
            connect_timeout,
            writer,
            writer_semaphore,
            readers,
            reader_semaphore,
            _marker: PhantomData,
        })
    }

    /// Extract the configuration for the given database name.
    fn get_config(rocket: &Rocket<Build>, db: &'static str) -> Result<Config> {
        Config::from(db, rocket).map_err(|e| {
            rocket::error!("Error configuring database {}: {}", db, e.to_string());
            Error::Configuration(Box::new(e))
        })
    }

    /// Get a connection pool with the given configuration, and run migrations on
    /// startup.
    fn get_pool_with_migrations_impl<T: RustEmbed>(
        rocket: &Rocket<Build>,
        db: &'static str,
        initializers: Vec<PoolInitializer>,
    ) -> Result<Self> {
        let config = Self::get_config(rocket, db)?;
        let pool = Self::new(&config, initializers)?;
        let migration_config = config.migrate;
        let pool_inner = pool
            .writer
            .as_ref()
            .cloned()
            .expect("internal invariant broken: self.pool is Some");
        let mut connection = pool_inner
            .get_timeout(pool.connect_timeout)
            .map_err(Error::ConnectionFailure)?;
        // TODO: Trace
        run_migrations::<T>(db, &migration_config, &mut connection).map_err(|e| {
            println!("Error running migrations: {e:?}");
            e
        })?;
        Ok(pool)
    }

    /// Fairing to attach to your rocket instance.
    pub fn fairing(
        fairing_name: &'static str,
        db: &'static str,
        initializers: Vec<PoolInitializer>,
    ) -> impl Fairing {
        AdHoc::try_on_ignite(fairing_name, move |rocket| async move {
            match Self::get_config(&rocket, db) {
                Ok(config) => Ok(rocket.manage(Self::new(&config, initializers))),
                Err(_) => Err(rocket),
            }
        })
    }

    /// Fairing to attach to your rocket instance, which will run migrations on startup.
    pub fn fairing_with_migrations<T: RustEmbed>(
        fairing_name: &'static str,
        db: &'static str,
        initializers: Vec<PoolInitializer>,
    ) -> impl Fairing {
        AdHoc::try_on_ignite(fairing_name, move |rocket| async move {
            run_blocking(move || {
                match Self::get_pool_with_migrations_impl::<T>(&rocket, db, initializers) {
                    Ok(pool) => Ok(rocket.manage(pool)),
                    Err(_) => Err(rocket),
                }
            })
            .await
        })
    }

    /// Helper method for getting a connection of a given type.
    async fn get_conn_inner<C>(
        connect_timeout: Duration,
        semaphore: Arc<Semaphore>,
        pool: &Option<Pool<SqliteConnectionManager>>,
    ) -> Result<C>
    where
        C: From<ConnectionHolder>,
    {
        let permit = if let Ok(permit) = timeout(connect_timeout, semaphore.acquire_owned()).await {
            permit.expect("internal invariant broken: semaphore should not be closed")
        } else {
            rocket::error!("database connection retrieval timed out");
            return Err(Error::ConnectionPermitRetrievalTimeout);
        };

        let pool = pool
            .as_ref()
            .cloned()
            .expect("internal invariant broken: self.pool is Some");

        match run_blocking(move || pool.get_timeout(connect_timeout)).await {
            Ok(c) => Ok(ConnectionHolder {
                connection: Arc::new(Mutex::new(Some(c))),
                permit: Some(permit),
            }
            .into()),
            Err(e) => {
                rocket::error!("failed to get a database connection: {}", e);
                Err(Error::ConnectionFailure(e))
            }
        }
    }

    /// Get a read connection.
    pub(crate) async fn get_read(&self) -> Result<ReadConnection<DB>> {
        Self::get_conn_inner(
            self.connect_timeout,
            Arc::clone(&self.reader_semaphore),
            &self.readers,
        )
        .await
    }

    /// Get a write connection.
    pub(crate) async fn get_write(
        &self,
        _authorization: WriteAuthorization,
    ) -> Result<WriteConnection<DB>> {
        Self::get_conn_inner(
            self.connect_timeout,
            Arc::clone(&self.writer_semaphore),
            &self.writer,
        )
        .await
    }

    /// Get a connector for this pool.
    #[inline]
    pub const fn get(&self) -> Connector<'_, DB> {
        Connector { pool: self }
    }

    /// Get a connector from the rocket instance
    #[inline]
    pub fn get_one<P: Phase>(rocket: &Rocket<P>) -> Option<Connector<'_, DB>> {
        rocket.state::<Self>().map_or_else(
            || {
                rocket::error!(
                    "missing database fairing for `{}`",
                    std::any::type_name::<DB>()
                );
                None
            },
            |pool| Some(pool.get()),
        )
    }

    /// Get a read-only connection from the pool and run the provided function
    /// against the connection
    pub async fn connect_and_read<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> R + Send,
        R: Send,
    {
        Ok(self.get_read().await?.run(f).await)
    }

    /// Get a read-only connection from the pool and run the provided function against
    /// the connection inside a transaction
    pub async fn connect_and_read_with_transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        Ok(self.get_read().await?.run_with_transaction(f).await)
    }

    /// Get a write connection from the pool and run the provided function against
    /// the connection inside a transaction
    pub async fn connect_and_write<F, R>(
        &self,
        authorization: WriteAuthorization,
        f: F,
    ) -> Result<R>
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        Ok(self.get_write(authorization).await?.run(f).await)
    }

    /// Get the pool from the rocket instance
    #[inline]
    pub fn get_pool<P: Phase>(rocket: &Rocket<P>) -> Option<&Self> {
        rocket.state::<Self>()
    }
}

impl<DB> Drop for ConnectionPool<DB> {
    fn drop(&mut self) {
        let writer = self.writer.take();
        let readers = self.readers.take();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(move || {
                drop(writer);
                drop(readers);
            });
        }
    }
}
