pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

#[derive(thiserror::Error, Debug)]
pub enum MigrationError {
    #[error("Versions must be incrementing integers starting from 1: Expected {0}, got {1}")]
    WrongVersionNumber(usize, usize),
    #[error("Single migration must end with just .sql, got path {0}")]
    WrongTypeForSingleMigration(String),
    #[error("Reversible Migration is missing the up migration! Got {0} instead")]
    ReversibleMigrationMissingUp(String),
    #[error("Reversible Migration is missing the down migration! Got {0} instead")]
    ReversibleMigrationMissingDown(String),
    #[error("Too many migrations for version {0}! Expected at most 2, got {1}")]
    TooManyMigrationsForVersion(usize, usize),
    #[error("Missing source for migration at path {0}")]
    MissingMigrationSource(String),
    #[error("Rusqlite migration: {0:?}")]
    RusqliteMigration(#[from] rusqlite_migration::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("rusqlite: {0:?}")]
    Rusqlite(#[from] rusqlite::Error),
    #[error("serde_rusqlite: {0:?}")]
    SerdeRusqlite(#[from] serde_rusqlite::Error),
    #[error("connection permit retrieval timed out")]
    ConnectionPermitRetrievalTimeout,
    #[error("connection failure: {0:?}")]
    ConnectionFailure(r2d2::Error),
    #[error("pool creation: {0:?}")]
    PoolCreation(r2d2::Error),
    #[error("Invalid configuration: {0:?}")]
    Configuration(BoxDynError),
    #[error("Migration: {0:?}")]
    Migration(#[from] MigrationError),
    #[error("Database fairing not set up for {0}")]
    MissingDatabaseFairing(String),
    #[error("Authorization not provided when fetching connection")]
    Unauthorized,
}
