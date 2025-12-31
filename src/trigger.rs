//! # Trigger Module
//!
//! This module handles automatic index synchronization via PostgreSQL triggers.
//!
//! ## Transaction Safety
//! This module uses transaction-aware buffered writes. Operations are queued
//! during the trigger execution and only committed when the PostgreSQL
//! transaction commits. If the transaction is rolled back, all pending
//! operations are discarded.

use pgrx::prelude::*;

use crate::writer;

/// Get list of indexed columns for a table
fn get_indexed_columns_for_table(
    table_name: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut columns = Vec::new();

    Spi::connect(|client| {
        let query = format!(
            "SELECT column_name, pk_column FROM pdb_index_metadata WHERE table_name = '{}'",
            table_name
        );

        client.select(&query, None, &[])?.for_each(|row| {
            if let (Ok(Some(col)), Ok(Some(pk_col))) = (row.get::<String>(1), row.get::<String>(2))
            {
                columns.push((col, pk_col));
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    Ok(columns)
}

/// Trigger function for automatic index synchronization
///
/// This trigger is automatically created when you call `create_bm25_index()`.
/// It keeps the Tantivy index in sync with PostgreSQL table changes.
///
/// # Operations:
/// - INSERT: Queues document addition (committed on transaction commit)
/// - UPDATE: Queues document deletion and addition
/// - DELETE: Queues document deletion
///
/// # Transaction Safety:
/// All operations are buffered until the PostgreSQL transaction commits.
/// If the transaction is rolled back, no changes are made to the index.
///
/// # Primary Key Correlation:
/// Documents use the table's primary key for identification, allowing
/// easy JOINs between search results and the original table.
#[pg_trigger]
pub fn pdb_sync_trigger<'a>(
    trigger: &'a PgTrigger<'a>,
) -> Result<Option<PgHeapTuple<'a, impl WhoAllocated>>, Box<dyn std::error::Error>> {
    let table_name = trigger.table_name()?.to_string();
    let operation = trigger.op()?;

    // Get all indexed columns for this table (with their PK column)
    let indexed_columns = get_indexed_columns_for_table(&table_name)?;

    // Process each indexed column
    for (column_name, pk_column) in indexed_columns {
        match operation {
            PgTriggerOperation::Insert => {
                if let Some(new_row) = trigger.new() {
                    // Get primary key value
                    let pk = new_row
                        .get_by_name::<i64>(&pk_column)?
                        .ok_or_else(|| format!("PK column {} not found or null", pk_column))?;

                    // Get content
                    let content = new_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or_else(|| format!("column {} not found", column_name))?;

                    // Queue the add operation (will be committed with the transaction)
                    writer::queue_add(&table_name, &column_name, pk, content)?;
                }
            }

            PgTriggerOperation::Update => {
                if let (Some(old_row), Some(new_row)) = (trigger.old(), trigger.new()) {
                    // Get old and new PK values
                    let old_pk = old_row
                        .get_by_name::<i64>(&pk_column)?
                        .ok_or_else(|| format!("old PK column {} not found or null", pk_column))?;
                    let new_pk = new_row
                        .get_by_name::<i64>(&pk_column)?
                        .ok_or_else(|| format!("new PK column {} not found or null", pk_column))?;

                    let new_content = new_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or_else(|| format!("new column {} not found", column_name))?;

                    // Delete old document and add updated version
                    writer::queue_delete(&table_name, &column_name, old_pk)?;
                    writer::queue_add(&table_name, &column_name, new_pk, new_content)?;
                }
            }

            PgTriggerOperation::Delete => {
                if let Some(old_row) = trigger.old() {
                    // Get primary key value
                    let pk = old_row
                        .get_by_name::<i64>(&pk_column)?
                        .ok_or_else(|| format!("PK column {} not found or null", pk_column))?;

                    // Queue the delete operation
                    writer::queue_delete(&table_name, &column_name, pk)?;
                }
            }

            PgTriggerOperation::Truncate => {
                // Truncate is not supported for FOR EACH ROW triggers
                // This should never happen, but handle gracefully
            }
        }
    }

    // Return the new row (or None for DELETE)
    Ok(trigger.new())
}
