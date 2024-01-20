use crate::holder::ConnectionHolder;

use std::marker::PhantomData;

use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, Transaction, TransactionBehavior};

/// A read-only connection to the database.
pub struct ReadConnection<DB> {
    holder: ConnectionHolder,
    _marker: PhantomData<fn() -> DB>,
}

impl<DB> From<ConnectionHolder> for ReadConnection<DB> {
    fn from(holder: ConnectionHolder) -> Self {
        Self {
            holder,
            _marker: PhantomData,
        }
    }
}

impl<DB: 'static> ReadConnection<DB> {

    /// Run the provided function against the connection
    #[inline]
    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Connection) -> R + Send,
        R: Send,
    {
        let with_connection =
            move |connection: &mut PooledConnection<SqliteConnectionManager>| f(connection);
        self.holder.run(with_connection).await
    }

    /// Run the provided function against the connection inside a transaction
    #[inline]
    pub async fn run_with_transaction<F, R>(&self, f: F) -> R
    where
        F: FnOnce(Transaction) -> R + Send,
        R: Send,
    {
        let with_transaction = move |connection: &mut PooledConnection<SqliteConnectionManager>| {
            // TODO: Better error handling
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .expect("internal invariant broken: couldn't create a transaction");
            f(transaction)
        };
        self.holder.run(with_transaction).await
    }
}

crate::define_from_request_for_gettable_connection!(ReadConnection, get_read);
crate::define_sentinel_for_gettable_connection!(ReadConnection);
