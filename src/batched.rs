use crate::query::values_clause;

use itertools::Itertools;
use rusqlite::{CachedStatement, ToSql, Transaction};
use serde::{de::DeserializeOwned, Serialize};
use serde_rusqlite::{columns_from_statement, from_row_with_columns, PositionalSliceSerializer};

/// Function that takes a values clause (e.g. VALUES (?,?,?)) and returns a query utilizing it.
type CreateQueryWithValuesClause = dyn Fn(String) -> String;
type Result<T, E = rusqlite::Error> = anyhow::Result<T, E>;

// TODO: Check based on sqlite limits feature if enabled
// from https://www.sqlite.org/limits.html point #9
const PARAMS_LIMIT: usize = 0x7FFE;

// TODO: Verify we don't exceed limits
/// A helper for splitting up arbtirary sized bulk inserts into smaller batches
/// which can be executed against a connection.
pub struct BatchedBulkValuesClause {
    query_creator: Box<CreateQueryWithValuesClause>,
    pre_params: Vec<Box<dyn ToSql>>,
    post_params: Vec<Box<dyn ToSql>>,
}

impl BatchedBulkValuesClause {
    /// Create a new [`BatchedBulkValuesClause`] with the given query creator.
    pub fn new(query_creator: Box<CreateQueryWithValuesClause>) -> Self {
        Self {
            query_creator,
            pre_params: vec![],
            post_params: vec![],
        }
    }

    /// Helper to serialize a value into a vector of boxed [`ToSql`]s.
    fn serialize_into<T: Serialize>(out: &mut Vec<Box<dyn ToSql>>, data: &T) -> Result<()> {
        out.extend(
            data.serialize(PositionalSliceSerializer::default())
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        );
        Ok(())
    }

    /// Binds params to come before the rows to be inserted. Useful when the query has
    /// some fixed parameters
    pub fn bind_pre<T: Serialize>(&mut self, params: &T) -> Result<()> {
        Self::serialize_into(&mut self.pre_params, params)
    }

    /// Binds params to come after the rows to be inserted. Useful when the query has
    /// some fixed parameters
    pub fn bind_post<T: Serialize>(&mut self, params: &T) -> Result<()> {
        Self::serialize_into(&mut self.post_params, params)
    }

    /// Computes the column count and batch size for the given row.
    fn compute_column_count_and_batch_size<T: Serialize>(
        &self,
        batch_size: usize,
        row: &T,
    ) -> Result<(usize, usize)> {
        let mut serialized_row = vec![];
        Self::serialize_into(&mut serialized_row, row)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let column_count = serialized_row.len();
        let max_batch_size =
            (PARAMS_LIMIT - (self.pre_params.len() + self.post_params.len())) / column_count;
        let batch_size = batch_size.min(max_batch_size);
        Ok((column_count, batch_size))
    }

    // We use drain to avoid reallocating serialized_row
    /// Creates a [`CachedStatement`] which can be reused for each batch.
    #[allow(clippy::iter_with_drain)]
    fn create<'t, T: Serialize>(
        &self,
        transaction: &'t Transaction,
        row_count: usize,
        column_count: usize,
        rows: impl Iterator<Item = T>,
    ) -> Result<CachedStatement<'t>> {
        let clause = values_clause(column_count, row_count);
        let query = (self.query_creator)(clause);
        let mut index = 1;
        let mut statement = transaction.prepare_cached(&query)?;
        for param in &self.pre_params {
            statement.raw_bind_parameter(index, param)?;
            index += 1;
        }
        let mut serialized_row = Vec::with_capacity(column_count);
        for row in rows {
            Self::serialize_into(&mut serialized_row, &row)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            for param in serialized_row.drain(..) {
                statement.raw_bind_parameter(index, param)?;
                index += 1;
            }
            serialized_row.clear();
        }
        for param in &self.post_params {
            statement.raw_bind_parameter(index, param)?;
            index += 1;
        }
        Ok(statement)
    }

    /// Executes an insert against the given transaction, processing `batch_size` rows at a time
    /// from the given iterator, which has the total number of rows specified by `row_count`.
    /// Returns the number of rows modified.
    pub fn execute<Input: Serialize>(
        self,
        transaction: &Transaction,
        row_count: usize,
        batch_size: usize,
        rows: impl Iterator<Item = Input>,
    ) -> Result<usize> {
        if row_count == 0 {
            return Ok(0);
        }
        let mut rows = rows.peekable();
        let (column_count, batch_size) = match rows.peek() {
            None => return Ok(0),
            Some(row) => self.compute_column_count_and_batch_size(batch_size, row)?,
        };
        let mut consumed = 0;
        let mut modified = 0;
        for chunk in &rows.chunks(batch_size) {
            let this_batch_size = if (consumed + batch_size) >= row_count {
                row_count - consumed
            } else {
                batch_size
            };
            let mut statement = self.create(transaction, this_batch_size, column_count, chunk)?;
            modified += statement.raw_execute()?;
            if this_batch_size < batch_size {
                statement.discard();
            }
            consumed += this_batch_size;
        }

        Ok(modified)
    }

    // TODO: See if we need a streaming version (will be hard)
    /// Executes a query against the given transaction, processing `batch_size` rows at a time
    /// from the given iterator, which has the total number of rows specified by `row_count`.
    /// Returns the output of the query.
    pub fn query<Input: Serialize, Output: DeserializeOwned>(
        self,
        transaction: &Transaction,
        row_count: usize,
        batch_size: usize,
        rows: impl Iterator<Item = Input>,
    ) -> Result<Vec<Output>> {
        if row_count == 0 {
            return Ok(vec![]);
        }
        let mut rows = rows.peekable();
        let (column_count, batch_size) = match rows.peek() {
            None => return Ok(vec![]),
            Some(row) => self.compute_column_count_and_batch_size(batch_size, row)?,
        };
        let mut consumed = 0;
        let mut output = Vec::with_capacity(row_count);
        let mut columns = vec![];
        for chunk in &rows.chunks(batch_size) {
            let this_batch_size = if (consumed + batch_size) >= row_count {
                row_count - consumed
            } else {
                batch_size
            };
            let mut statement = self.create(transaction, this_batch_size, column_count, chunk)?;
            if columns.is_empty() {
                columns = columns_from_statement(&statement);
            }
            output.extend(
                statement
                    .raw_query()
                    .and_then(|row| {
                        // TODO: Proper error
                        from_row_with_columns::<Output>(row, &columns).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Null,
                                Box::new(e),
                            )
                        })
                    })
                    .collect::<Result<Vec<Output>, rusqlite::Error>>()?
                    .into_iter(),
            );
            if this_batch_size < batch_size {
                statement.discard();
            }
            consumed += this_batch_size;
        }
        Ok(output)
    }
}
