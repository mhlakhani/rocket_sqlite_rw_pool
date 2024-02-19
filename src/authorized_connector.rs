use crate::{ConnectionPool, Error, ReadConnection, WriteAuthorization, WriteConnection};

use rocket::{
    http::Status,
    request::{FromRequest, Outcome, Request},
};
use rusqlite::{Connection, Transaction};

type Result<T, E = Error> = anyhow::Result<T, E>;

/// Struct representing an authorized connector.
/// This connector is pre-authorized, meaning it can create write connections directly.
#[derive(Clone)]
pub struct AuthorizedConnector<'pool, DB> {
    pub(crate) pool: &'pool ConnectionPool<DB>,
    pub(crate) authorization: WriteAuthorization,
}

impl<'pool, DB: 'static> AuthorizedConnector<'pool, DB> {
    /// Get a read-only connection from the pool.
    pub async fn read(&self) -> Result<ReadConnection<DB>> {
        self.pool.get_read().await
    }

    /// Get a write connection from the pool.
    pub async fn write(&self) -> Result<WriteConnection<DB>> {
        self.pool.get_write(self.authorization.clone()).await
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
    pub async fn connect_and_write<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        self.pool
            .connect_and_write(self.authorization.clone(), f)
            .await
    }
}

/// Support downgrading to allow passing this to methods that only need a read-only connection
impl<'pool, DB> From<AuthorizedConnector<'pool, DB>> for crate::Connector<'pool, DB> {
    fn from(authorized: AuthorizedConnector<'pool, DB>) -> Self {
        Self {
            pool: authorized.pool,
        }
    }
}

crate::define_sentinel_for_pool_holder!(AuthorizedConnector);

// We intentionally do not implement FromRequest from the macro,
// as we need to check for authentication here.
/// Extractor for an authorized connector, checks for the request having passed
/// CSRF checks.
#[async_trait::async_trait]
impl<'r, DB: 'static> FromRequest<'r> for AuthorizedConnector<'r, DB> {
    type Error = Error;

    #[inline]
    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.guard::<WriteAuthorization>().await {
            Outcome::Success(authorization) => {
                request.rocket().state::<ConnectionPool<DB>>().map_or_else(
                    || {
                        rocket::error!(
                            "Missing database fairing for `{}`",
                            std::any::type_name::<DB>()
                        );
                        Outcome::Error((
                            Status::InternalServerError,
                            Error::MissingDatabaseFairing(std::any::type_name::<DB>().to_owned()),
                        ))
                    },
                    |pool| {
                        Outcome::Success(AuthorizedConnector {
                            pool,
                            authorization,
                        })
                    },
                )
            }
            Outcome::Error((status, _)) => Outcome::Error((status, Error::Unauthorized)),
            Outcome::Forward(_) => {
                Outcome::Error((Status::InternalServerError, Error::Unauthorized))
            }
        }
    }
}
