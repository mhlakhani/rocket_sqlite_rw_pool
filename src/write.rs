use crate::{holder::ConnectionHolder, ConnectionPool, Error, WriteAuthorization};

use std::marker::PhantomData;

use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rocket::{
    http::Status,
    outcome::IntoOutcome,
    request::{FromRequest, Outcome, Request},
};
use rusqlite::{Transaction, TransactionBehavior};

/// A write connection to the database.
pub struct WriteConnection<DB> {
    holder: ConnectionHolder,
    _marker: PhantomData<fn() -> DB>,
}

impl<DB> From<ConnectionHolder> for WriteConnection<DB> {
    fn from(holder: ConnectionHolder) -> Self {
        Self {
            holder,
            _marker: PhantomData,
        }
    }
}

impl<DB: 'static> WriteConnection<DB> {
    /// Run the provided function against the connection inside a transaction
    #[inline]
    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        let with_transaction = move |connection: &mut PooledConnection<SqliteConnectionManager>| {
            // TODO: Better error handling
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("internal invariant broken: couldn't create a transaction");
            f(transaction)
        };
        self.holder.run(with_transaction).await
    }
}

crate::define_sentinel_for_gettable_connection!(WriteConnection);

// We intentionally do not implement FromRequest from the macro,
// as we need to check for authentication here.
#[async_trait::async_trait]
impl<'r, DB: 'static> FromRequest<'r> for WriteConnection<DB> {
    type Error = Error;

    #[inline]
    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.guard::<WriteAuthorization>().await {
            Outcome::Success(authorization) => {
                if let Some(pool) = request.rocket().state::<ConnectionPool<DB>>() {
                    pool.get_write(authorization)
                        .await
                        .or_forward(Status::ServiceUnavailable)
                } else {
                    rocket::error!(
                        "Missing database fairing for `{}`",
                        std::any::type_name::<DB>()
                    );
                    Outcome::Error((
                        Status::InternalServerError,
                        Error::MissingDatabaseFairing(std::any::type_name::<DB>().to_owned()),
                    ))
                }
            }
            Outcome::Error((status, _)) => Outcome::Error((status, Error::Unauthorized)),
            Outcome::Forward(_) => {
                Outcome::Error((Status::InternalServerError, Error::Unauthorized))
            }
        }
    }
}
