#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::significant_drop_tightening,
    clippy::struct_field_names
)]

mod auth;
mod authorized_connector;
mod batched;
mod config;
mod connector;
mod error;
mod holder;
mod macros;
mod migration;
mod pool;
mod pragmas;
mod query;
mod read;
mod util;
mod write;

pub use inventory;
pub use paste;

pub use auth::WriteAuthorization;
pub use authorized_connector::AuthorizedConnector;
pub use batched::BatchedBulkValuesClause;
pub use connector::Connector;
pub use error::Error;
pub use pool::{ConnectionPool, PoolInitializer, PoolInitializerFn};
pub use query::*;
pub use read::ReadConnection;
pub use rust_embed;
pub use write::WriteConnection;
