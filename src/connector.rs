use crate::{
    AuthorizedConnector, ConnectionPool, Error, ReadConnection, WriteAuthorization, WriteConnection,
};

use rusqlite::{Connection, Transaction};

type Result<T, E = Error> = anyhow::Result<T, E>;

/// Connector struct that can be used to get read or write connections from the pool.
#[derive(Clone)]
pub struct Connector<'pool, DB> {
    pub(crate) pool: &'pool ConnectionPool<DB>,
}

impl<'pool, DB: 'static> Connector<'pool, DB> {

    /// Get a read-only connection from the pool.
    pub async fn read(&self) -> Result<ReadConnection<DB>> {
        self.pool.get_read().await
    }

    /// Upgrade to an [`AuthorizedConnector`] with the given authorization.
    pub const fn authorize(
        &self,
        authorization: WriteAuthorization,
    ) -> AuthorizedConnector<'pool, DB> {
        AuthorizedConnector {
            pool: self.pool,
            authorization,
        }
    }

    /// Get a write connection from the pool given an authorization.
    pub async fn authorized_write(
        &self,
        authorization: WriteAuthorization,
    ) -> Result<WriteConnection<DB>> {
        self.pool.get_write(authorization).await
    }

    // TODO: Investigate caching the read connection
    /// Get a read-only connection from the pool and run the provided function
    /// against the connection
    pub async fn connect_and_read<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> R + Send,
        R: Send,
    {
        self.pool.connect_and_read(f).await
    }

    /// Get a read-only connection from the pool and run the provided function against
    /// the connection inside a transaction
    pub async fn connect_and_read_with_transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        self.pool.connect_and_read_with_transaction(f).await
    }

    /// Get a write connection from the pool and run the provided function against
    /// the connection inside a transaction
    pub async fn connect_and_write<F, R>(&self, auth: WriteAuthorization, f: F) -> Result<R>
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        self.pool.connect_and_write(auth, f).await
    }
}

crate::define_from_request_for_pool_holder!(Connector);
crate::define_sentinel_for_pool_holder!(Connector);
