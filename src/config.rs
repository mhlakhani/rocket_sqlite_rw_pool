use crate::pragmas::Pragmas;

use rocket::{
    figment::{providers::Serialized, Error, Figment},
    Build, Rocket,
};
use serde::Deserialize;

/// Configuration for migrations
#[derive(Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationConfig {
    /// Target version to migrate to. If not specified, defaults to the latest.
    /// If specified, will migrate to version `n`
    #[serde(default)]
    pub(crate) to: Option<usize>,
    /// If set, first migrate to this version before migrating to the `to` version.
    /// Helpful for rolling back some changes once
    #[serde(default)]
    pub(crate) first_to: Option<usize>,
}

// TODO: Think about shared cache, statement cache,
// Reuses the same configurations as what's provided by rocket itself.
/// Configuration for a database.
/// This struct holds all the necessary configuration options for a database connection.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The URL of the database to connect to.
    pub(crate) url: String,

    /// Pragmas to be applied to the database connection.
    pub(crate) pragmas: Pragmas,

    /// The minimum number of read connections to maintain in the connection pool.
    /// Note that this only applies to read connections, as write connections are limited to one.
    pub(crate) min_read_connections: Option<u32>,

    /// The maximum number of read connections allowed in the connection pool.
    pub(crate) max_read_connections: u32,

    /// The maximum amount of time (in milliseconds) to wait when trying to connect to the database before giving up.
    pub(crate) connect_timeout: u64,

    /// The maximum amount of time (in milliseconds) a connection can remain idle in the pool before it is closed.
    pub(crate) idle_timeout: Option<u64>,

    /// The maximum amount of time (in milliseconds) to wait when trying to execute a query on the database before giving up.
    pub(crate) busy_timeout: Option<u64>,

    /// Configuration for database migrations.
    /// This includes the version to migrate to and an optional first version to migrate to before the final version.
    #[serde(default)]
    pub(crate) migrate: MigrationConfig,
}

impl Config {
    pub(crate) fn from(db_name: &str, rocket: &Rocket<Build>) -> Result<Self, Error> {
        Self::figment(db_name, rocket).extract::<Self>()
    }

    fn figment(db_name: &str, rocket: &Rocket<Build>) -> Figment {
        let db_key = format!("databases.{db_name}");
        let default_max_read_connections = rocket
            .figment()
            .extract_inner::<u32>(rocket::Config::WORKERS)
            .map(|workers| workers * 4)
            .ok();

        let figment = Figment::from(rocket.figment())
            .focus(&db_key)
            .join(Serialized::default("connect_timeout", 5_i32))
            .join(Serialized::default("pragmas", Pragmas::default()));

        match default_max_read_connections {
            Some(max_read_connections) => figment.join(Serialized::default(
                "max_read_connections",
                max_read_connections,
            )),
            None => figment,
        }
    }
}
