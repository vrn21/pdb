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
pub(crate) fn hash_ctid_string(ctid: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    ctid.hash(&mut hasher);
    hasher.finish()
}

/// Ensures the metadata table exists for tracking indexed columns
fn ensure_metadata_table() -> Result<(), Box<dyn std::error::Error>> {
    Spi::run(
        "
        CREATE TABLE IF NOT EXISTS pdb_index_metadata (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW(),
            PRIMARY KEY (table_name, column_name)
        )
    ",
    )?;
    Ok(())
}

/// Register an index in the metadata table
fn register_index_metadata(
    table_name: &str,
    column_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_metadata_table()?;

    Spi::run(&format!(
        "INSERT INTO pdb_index_metadata (table_name, column_name) VALUES ('{}', '{}')",
        table_name, column_name
    ))?;

    Ok(())
}

/// Unregister an index from the metadata table
fn unregister_index_metadata(
    table_name: &str,
    column_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    Spi::run(&format!(
        "DELETE FROM pdb_index_metadata WHERE table_name = '{}' AND column_name = '{}'",
        table_name, column_name
    ))?;

    Ok(())
}

/// Create a sync trigger for automatic index updates
fn create_sync_trigger(
    table_name: &str,
    column_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let trigger_name = format!("pdb_sync_{}_{}", table_name, column_name);

    Spi::run(&format!(
        "CREATE TRIGGER {} 
         AFTER INSERT OR UPDATE OR DELETE ON {} 
         FOR EACH ROW EXECUTE FUNCTION pdb_sync_trigger()",
        trigger_name, table_name
    ))?;

    Ok(())
}

/// Drop a sync trigger
fn drop_sync_trigger(
    table_name: &str,
    column_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let trigger_name = format!("pdb_sync_{}_{}", table_name, column_name);

    Spi::run(&format!(
        "DROP TRIGGER IF EXISTS {} ON {}",
        trigger_name, table_name
    ))?;

    Ok(())
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

    // Check if index already exists
    if index_path.exists() {
        return Err(format!(
            "Index already exists on {}.{}. Drop it first.",
            table_name, column_name
        )
        .into());
    }

    std::fs::create_dir_all(&index_path)?;

    // 3. Create Tantivy index
    let index = Index::create_in_dir(&index_path, schema)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?; // 50MB buffer

    // 4. Scan table and add documents
    let indexed_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let count_clone = indexed_count.clone();

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
                    count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    // 5. Commit
    writer.commit()?;

    let count = indexed_count.load(std::sync::atomic::Ordering::Relaxed);

    // 6. Register in metadata table and create trigger
    register_index_metadata(table_name, column_name)?;
    create_sync_trigger(table_name, column_name)?;

    pgrx::info!(
        "✓ Created BM25 index on {}.{} ({} documents) with automatic sync",
        table_name,
        column_name,
        count
    );
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
        // 1. Drop trigger first
        drop_sync_trigger(table_name, column_name)?;

        // 2. Unregister from metadata
        unregister_index_metadata(table_name, column_name)?;

        // 3. Remove index files
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
