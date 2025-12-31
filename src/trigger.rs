//! # Trigger Module
//!
//! This module handles automatic index synchronization via PostgreSQL triggers.

use pgrx::prelude::*;
use std::path::PathBuf;
use tantivy::{doc, Index, IndexWriter, Term};

/// Get the path where the index is stored
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Get list of indexed columns for a table
fn get_indexed_columns_for_table(
    table_name: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut columns = Vec::new();

    Spi::connect(|client| {
        let query = format!(
            "SELECT column_name FROM pdb_index_metadata WHERE table_name = '{}'",
            table_name
        );

        client.select(&query, None, &[])?.for_each(|row| {
            if let Ok(Some(col)) = row.get::<String>(1) {
                columns.push(col);
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    Ok(columns)
}

/// Generate a unique document ID from row content
/// Since ctid changes on UPDATE and isn't accessible from trigger tuples,
/// we hash the content itself to generate a stable-ish ID
fn generate_doc_id(content: &str, table_name: &str, column_name: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    table_name.hash(&mut hasher);
    column_name.hash(&mut hasher);
    content.hash(&mut hasher);
    hasher.finish()
}

/// Add a document to the Tantivy index
fn add_document_to_index(
    table_name: &str,
    column_name: &str,
    row_id: u64,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let index_path = get_index_path(table_name, column_name);
    let index = Index::open_in_dir(&index_path)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let schema = index.schema();
    let row_id_field = schema.get_field("row_id")?;
    let content_field = schema.get_field("content")?;

    writer.add_document(doc!(
        row_id_field => row_id,
        content_field => content
    ))?;

    writer.commit()?;
    Ok(())
}

/// Remove a document from the Tantivy index
fn remove_document_from_index(
    table_name: &str,
    column_name: &str,
    row_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let index_path = get_index_path(table_name, column_name);
    let index = Index::open_in_dir(&index_path)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let schema = index.schema();
    let row_id_field = schema.get_field("row_id")?;

    let term = Term::from_field_u64(row_id_field, row_id);

    writer.delete_term(term);
    writer.commit()?;

    Ok(())
}

/// Trigger function for automatic index synchronization
///
/// This trigger is automatically created when you call `create_bm25_index()`.
/// It keeps the Tantivy index in sync with PostgreSQL table changes.
///
/// # Operations:
/// - INSERT: Adds new document to index
/// - UPDATE: Removes old document, adds updated document
/// - DELETE: Removes document from index
///
/// # Note on Row IDs:
/// Since ctid is not accessible from trigger tuples and changes on UPDATE,
/// we generate document IDs by hashing the content. This means:
/// - Same content = same ID (potential duplicates)
/// - Updated content = different ID (old version auto-deleted by UPDATE logic)
#[pg_trigger]
pub fn pdb_sync_trigger<'a>(
    trigger: &'a PgTrigger<'a>,
) -> Result<Option<PgHeapTuple<'a, impl WhoAllocated>>, Box<dyn std::error::Error>> {
    let table_name = trigger.table_name()?.to_string();
    let operation = trigger.op()?;

    // Get all indexed columns for this table
    let indexed_columns = get_indexed_columns_for_table(&table_name)?;

    // Process each indexed column
    for column_name in indexed_columns {
        match operation {
            PgTriggerOperation::Insert => {
                if let Some(new_row) = trigger.new() {
                    let content = new_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or(format!("column {} not found", column_name))?;

                    let row_id = generate_doc_id(&content, &table_name, &column_name);
                    add_document_to_index(&table_name, &column_name, row_id, &content)?;
                }
            }

            PgTriggerOperation::Update => {
                if let (Some(old_row), Some(new_row)) = (trigger.old(), trigger.new()) {
                    let old_content = old_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or(format!("old column {} not found", column_name))?;
                    let new_content = new_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or(format!("new column {} not found", column_name))?;

                    // Remove old document and add updated one
                    let old_row_id = generate_doc_id(&old_content, &table_name, &column_name);
                    let new_row_id = generate_doc_id(&new_content, &table_name, &column_name);

                    remove_document_from_index(&table_name, &column_name, old_row_id)?;
                    add_document_to_index(&table_name, &column_name, new_row_id, &new_content)?;
                }
            }

            PgTriggerOperation::Delete => {
                if let Some(old_row) = trigger.old() {
                    let content = old_row
                        .get_by_name::<String>(&column_name)?
                        .ok_or(format!("column {} not found", column_name))?;

                    let row_id = generate_doc_id(&content, &table_name, &column_name);
                    remove_document_from_index(&table_name, &column_name, row_id)?;
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
