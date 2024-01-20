use std::{collections::HashMap, fmt::Write};

use rusqlite::Connection as RusqliteConnection;
use serde::{Deserialize, Serialize};

mod pragma_defaults {
    pub const fn page_size() -> usize {
        4096
    }

    pub fn locking_mode() -> String {
        "NORMAL".to_owned()
    }

    pub fn journal_mode() -> String {
        "WAL".to_owned()
    }

    pub fn foreign_keys() -> String {
        "ON".to_owned()
    }

    pub fn synchronous() -> String {
        "NORMAL".to_owned()
    }

    pub fn auto_vacuum() -> String {
        "NONE".to_owned()
    }
}

/// Pragmas to set on the database connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pragmas {
    #[serde(default)]
    pragmas: HashMap<String, String>,
    #[serde(default = "pragma_defaults::page_size")]
    page_size: usize,
    #[serde(default = "pragma_defaults::locking_mode")]
    locking_mode: String,
    #[serde(default = "pragma_defaults::journal_mode")]
    journal_mode: String,
    #[serde(default = "pragma_defaults::foreign_keys")]
    foreign_keys: String,
    #[serde(default = "pragma_defaults::synchronous")]
    synchronous: String,
    #[serde(default = "pragma_defaults::auto_vacuum")]
    auto_vacuum: String,
}

impl Default for Pragmas {
    fn default() -> Self {
        let pragmas = HashMap::new();
        let page_size = pragma_defaults::page_size();
        let locking_mode = pragma_defaults::locking_mode();
        let journal_mode = pragma_defaults::journal_mode();
        let foreign_keys = pragma_defaults::foreign_keys();
        let synchronous = pragma_defaults::synchronous();
        let auto_vacuum = pragma_defaults::auto_vacuum();
        Self {
            pragmas,
            page_size,
            locking_mode,
            journal_mode,
            foreign_keys,
            synchronous,
            auto_vacuum,
        }
    }
}

impl Pragmas {
    pub(crate) fn set(&self, connection: &RusqliteConnection) -> Result<(), rusqlite::Error> {
        let mut query = format!(
            r#"
PRAGMA page_size = {};
PRAGMA locking_mode = {};
PRAGMA journal_mode = {};
PRAGMA foreign_keys = {};
PRAGMA synchronous = {};
PRAGMA auto_vacuum = {};
"#,
            self.page_size,
            self.locking_mode,
            self.journal_mode,
            self.foreign_keys,
            self.synchronous,
            self.auto_vacuum
        );
        for (k, v) in &self.pragmas {
            let _ = writeln!(query, "PRAGMA {k} = {v};");
        }
        connection.execute_batch(&query)
    }
}
