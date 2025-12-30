//! # Index Management Module
//!
//! This module handles creating, dropping, and refreshing BM25 search indexes.

use pgrx::prelude::*;
use std::path::PathBuf;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter};

/// Build the Tantivy schema for our index
///
/// Two fields:
/// - row_id: u64 - converted from Postgres ctid
/// - content: text - the searchable text field
fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_u64_field("row_id", INDEXED | STORED);
    builder.add_text_field("content", TEXT);
    builder.build()
}

/// Get the path where the index is stored
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    // Get PGDATA directory
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Simple hash function for ctid strings
///
/// ctid format: "(block,offset)" e.g. "(0,1)"
/// We hash this string to get a u64 for use as Tantivy document ID
fn hash_ctid_string(ctid: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    ctid.hash(&mut hasher);
    hasher.finish()
}

/// Creates a BM25 search index on a text column.
///
/// # What this does:
/// 1. Creates a Tantivy index on disk
/// 2. Scans all rows from the table via SPI
/// 3. Adds each row to the index
/// 4. Commits the index
///
/// # SQL Usage:
/// ```sql
/// SELECT create_bm25_index('articles', 'body');
/// ```
///
/// # Arguments:
/// * `table_name` - Name of the table to index
/// * `column_name` - Name of the TEXT column to index
///
/// # Returns:
/// * `true` if index was created successfully
#[pg_extern]
pub fn create_bm25_index(
    table_name: &str,
    column_name: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Build schema
    let schema = build_schema();
    let row_id_field = schema.get_field("row_id").expect("row_id field missing");
    let content_field = schema.get_field("content").expect("content field missing");

    // 2. Create index directory
    let index_path = get_index_path(table_name, column_name);
    std::fs::create_dir_all(&index_path)?;

    // 3. Create Tantivy index
    let index = Index::create_in_dir(&index_path, schema)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?; // 50MB buffer

    // 4. Scan table and add documents
    let mut indexed_count = 0u64;
    Spi::connect(|client| {
        // Cast ctid to text for simpler extraction
        let query = format!("SELECT ctid::text, {} FROM {}", column_name, table_name);

        client.select(&query, None, &[])?.for_each(|row| {
            // Get ctid as text (format: "(block,offset)")
            if let Ok(Some(ctid_text)) = row.get::<String>(1) {
                // Get text content
                if let Ok(Some(txt)) = row.get::<String>(2) {
                    // Simple hash of ctid string for document ID
                    let id = hash_ctid_string(&ctid_text);

                    let _ = writer.add_document(doc!(
                        row_id_field => id,
                        content_field => txt
                    ));
                    indexed_count += 1;
                }
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    // 5. Commit
    writer.commit()?;

    pgrx::info!("Created BM25 index on {}.{}", table_name, column_name);
    Ok(true)
}

/// Drops (deletes) a BM25 search index.
///
/// # SQL Usage:
/// ```sql
/// SELECT drop_bm25_index('articles', 'body');
/// ```
#[pg_extern]
pub fn drop_bm25_index(
    table_name: &str,
    column_name: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let index_path = get_index_path(table_name, column_name);

    if index_path.exists() {
        std::fs::remove_dir_all(&index_path)?;
        pgrx::info!("Dropped BM25 index on {}.{}", table_name, column_name);
    } else {
        pgrx::warning!("Index does not exist: {}.{}", table_name, column_name);
    }

    Ok(true)
}

/// Manually refreshes (rebuilds) an index.
///
/// For now, this just drops and recreates the index.
/// Later, we'll make this smarter with incremental updates.
///
/// # SQL Usage:
/// ```sql
/// SELECT refresh_bm25_index('articles', 'body');
/// ```
#[pg_extern]
pub fn refresh_bm25_index(
    table_name: &str,
    column_name: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    drop_bm25_index(table_name, column_name)?;
    create_bm25_index(table_name, column_name)?;
    pgrx::info!("Refreshed BM25 index on {}.{}", table_name, column_name);
    Ok(true)
}
