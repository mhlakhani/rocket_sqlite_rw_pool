use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{de::DeserializeOwned, Serialize};
use serde_rusqlite::{columns_from_statement, from_row_with_columns, to_params};

/// Execute the given INSERT query against the given transaction with the given parameters.
pub fn execute_with_params<T: Serialize>(
    query: &str,
    transaction: &Transaction,
    params: &T,
) -> Result<usize, rusqlite::Error> {
    let mut statement = transaction.prepare_cached(query)?;
    let modified = statement.execute(
        to_params(params).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
    )?;
    Ok(modified)
}

/// Execute the given SELECT query against the given transaction with the given parameters, returning the result.
pub fn query_with_params<Input: Serialize, Output: DeserializeOwned>(
    query: &str,
    connection: &Connection,
    params: &Input,
) -> Result<Vec<Output>, rusqlite::Error> {
    let mut statement = connection.prepare_cached(query)?;
    let columns = columns_from_statement(&statement);
    let result = statement
        .query_and_then(
            to_params(params).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            // TODO: Proper error
            |row| {
                from_row_with_columns(row, &columns).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Null,
                        Box::new(e),
                    )
                })
            },
        )?
        .collect::<Result<Vec<Output>, rusqlite::Error>>()?;
    Ok(result)
}

/// Execute the given query (which takes no parameters) and return the result.
pub fn query_without_params<Output: DeserializeOwned>(
    query: &str,
    connection: &Connection,
) -> Result<Vec<Output>, rusqlite::Error> {
    let mut statement = connection.prepare_cached(query)?;
    let columns = columns_from_statement(&statement);
    let result = statement
        .query_and_then(
            (),
            // TODO: Proper error
            |row| {
                from_row_with_columns(row, &columns).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Null,
                        Box::new(e),
                    )
                })
            },
        )?
        .collect::<Result<Vec<Output>, rusqlite::Error>>()?;
    Ok(result)
}

/// Execute the given query and return the result, which can be at most one row.
pub fn query_optional_with_params<Input: Serialize, Output: DeserializeOwned>(
    query: &str,
    connection: &Connection,
    params: &Input,
) -> Result<Option<Output>, rusqlite::Error> {
    let mut statement = connection.prepare_cached(query)?;
    let columns = columns_from_statement(&statement);
    let result = statement
        .query_row(
            to_params(params).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            // TODO: Proper error
            |row| {
                from_row_with_columns(row, &columns).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Null,
                        Box::new(e),
                    )
                })
            },
        )
        .optional()?;
    Ok(result)
}

/// Run a query which should return exactly one row (with the given parameters)
pub fn query_single_with_params<Input: Serialize, Output: DeserializeOwned>(
    query: &str,
    connection: &Connection,
    params: &Input,
) -> Result<Output, rusqlite::Error> {
    match query_optional_with_params(query, connection, params) {
        Ok(Some(row)) => Ok(row),
        Ok(None) => Err(rusqlite::Error::QueryReturnedNoRows),
        Err(e) => Err(e),
    }
}

/// Returns a string of the form `VALUES (?,?,...),(?,?,...),...` with the given number of columns and rows.
/// # Panics
///
/// Will panic if cols or rows is 0
pub fn values_clause(cols: usize, rows: usize) -> String {
    assert_ne!(cols, 0);
    assert_ne!(rows, 0);
    let mut s = String::from("VALUES ");
    let single_row = {
        let mut r = String::from("(");
        r.push_str(&"?,".repeat(cols));
        // Remove trailing comma
        r.pop();
        r.push_str("),");
        r
    };
    s.push_str(&single_row.repeat(rows));
    // Remove trailing comma
    s.pop();
    s.push(' ');
    s
}
