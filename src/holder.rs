use std::sync::Arc;

use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use tokio::sync::{Mutex, OwnedSemaphorePermit};

/// A holder for a connection that will be released when dropped.
pub struct ConnectionHolder {
    pub(crate) connection: Arc<Mutex<Option<PooledConnection<SqliteConnectionManager>>>>,
    pub(crate) permit: Option<OwnedSemaphorePermit>,
}

impl ConnectionHolder {
    /// Run the provided function against the connection.
    #[inline]
    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut PooledConnection<SqliteConnectionManager>) -> R + Send,
        R: Send,
    {
        let connection = Arc::clone(&self.connection);
        let mut connection = connection.lock_owned().await;
        let conn = connection
            .as_mut()
            .expect("internal invariant broken: self.connection is Some");
        f(conn)
    }
}

impl Drop for ConnectionHolder {
    fn drop(&mut self) {
        // It is important that this inner Arc<Mutex<>> (or the OwnedMutexGuard
        // derived from it) never be a variable on the stack at an await point,
        // where Drop might be called at any time. This causes (synchronous)
        // Drop to be called from asynchronous code, which some database
        // wrappers do not or can not handle.
        let connection = Arc::clone(&self.connection);
        let permit = self.permit.take();

        // Since connection can't be on the stack in an async fn during an
        // await, we have to spawn a new blocking-safe thread...
        tokio::task::spawn_blocking(move || {
            // And then re-enter the runtime to wait on the async mutex, but in
            // a blocking fashion.
            let mut connection =
                tokio::runtime::Handle::current().block_on(async { connection.lock_owned().await });

            if let Some(conn) = connection.take() {
                drop(conn);
            }

            // Explicitly dropping the permit here so that it's only
            // released after the connection is.
            drop(permit);
        });
    }
}
