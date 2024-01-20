use crate::{config::MigrationConfig, error::MigrationError};

use itertools::Itertools;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use rust_embed::RustEmbed;

type Result<T, E = MigrationError> = anyhow::Result<T, E>;

/// Contents of a single migration
#[derive(Debug)]
enum MigrationContents {
    Simple(String),
    Reversible(String, String),
}

/// The type of a migration source
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MigrationSourceType {
    Simple,
    Up,
    Down,
}

impl MigrationSourceType {
    const fn extension(self) -> &'static str {
        match self {
            Self::Simple => ".sql",
            Self::Up => ".up.sql",
            Self::Down => ".down.sql",
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn run_migrations<T: RustEmbed>(
    db_name: &'static str,
    config: &MigrationConfig,
    connection: &mut Connection,
) -> Result<()> {
    let sources = T::iter().filter_map(|filename| {
        let parts = filename.splitn(2, '_').collect::<Vec<_>>();

        if parts.len() != 2
            || !std::path::Path::new(parts[1])
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("sql"))
        {
            return None;
        }

        let version: usize = parts[0].parse().ok()?;
        let rest = parts[1];

        if rest.ends_with(MigrationSourceType::Up.extension()) {
            Some((MigrationSourceType::Up, version, filename))
        } else if rest.ends_with(MigrationSourceType::Down.extension()) {
            Some((MigrationSourceType::Down, version, filename))
        } else if rest.ends_with(MigrationSourceType::Simple.extension()) {
            Some((MigrationSourceType::Simple, version, filename))
        } else {
            None
        }
    });
    let contents: Result<Vec<_>> = sources
        .into_iter()
        // version then type
        .sorted_by(|a, b| Ord::cmp(&(a.1, a.0), &(b.1, b.0)))
        .group_by(|source| source.1)
        .into_iter()
        .enumerate()
        .map(|(index, (version, entries))| {
            let entries: Vec<_> = entries.into_iter().collect();
            if version != (1 + index) {
                return Err(MigrationError::WrongVersionNumber(index + 1, version));
            }
            if entries.len() == 1 {
                if entries[0].0 != MigrationSourceType::Simple {
                    return Err(MigrationError::WrongTypeForSingleMigration(
                        entries[0].2.to_string(),
                    ));
                }
                Ok(MigrationContents::Simple(entries[0].2.to_string()))
            } else if entries.len() == 2 {
                if entries[0].0 != MigrationSourceType::Up {
                    return Err(MigrationError::ReversibleMigrationMissingUp(
                        entries[0].2.to_string(),
                    ));
                }
                if entries[1].0 != MigrationSourceType::Down {
                    return Err(MigrationError::ReversibleMigrationMissingDown(
                        entries[1].2.to_string(),
                    ));
                }
                Ok(MigrationContents::Reversible(
                    entries[0].2.to_string(),
                    entries[1].2.to_string(),
                ))
            } else {
                Err(MigrationError::TooManyMigrationsForVersion(
                    version,
                    entries.len(),
                ))
            }
        })
        .collect();
    // Load files
    let contents: Result<Vec<_>> = contents?
        .into_iter()
        .map(|contents| match contents {
            MigrationContents::Simple(path) => {
                let source = T::get(&path).ok_or(MigrationError::MissingMigrationSource(path))?;
                let sql = String::from_utf8_lossy(&source.data).to_string();
                Ok(MigrationContents::Simple(sql))
            }
            MigrationContents::Reversible(up_path, down_path) => {
                let up_source =
                    T::get(&up_path).ok_or(MigrationError::MissingMigrationSource(up_path))?;
                let up_sql = String::from_utf8_lossy(&up_source.data).to_string();
                let down_source =
                    T::get(&down_path).ok_or(MigrationError::MissingMigrationSource(down_path))?;
                let down_sql = String::from_utf8_lossy(&down_source.data).to_string();
                Ok(MigrationContents::Reversible(up_sql, down_sql))
            }
        })
        .collect();
    // TODO: Avoid the copy by holding the EmbeddedFile in a vec
    let contents = contents?;
    let ms: Vec<M> = contents
        .iter()
        .map(|contents| match contents {
            MigrationContents::Simple(sql) => M::up(sql),
            MigrationContents::Reversible(up, down) => M::up(up).down(down),
        })
        .collect();
    let migrations = Migrations::new(ms);
    let mut current_version: usize = migrations.current_version(connection)?.into();
    if let Some(to) = config.first_to {
        if to != current_version {
            println!("Migrating {db_name} to version {to}.");
            migrations.to_version(connection, to)?;
            current_version = to;
        }
    }
    if let Some(to) = config.to {
        println!("Migrating {db_name} to version {to}.");
        migrations.to_version(connection, to)?;
    } else if current_version < contents.len() {
        println!(
            "Migrating {} to latest version ({}).",
            db_name,
            contents.len()
        );
        migrations.to_latest(connection)?;
    } else {
        // Nothing to do here, moving on...
    }
    Ok(())
}
