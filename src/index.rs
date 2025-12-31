//! # Index Management Module
//!
//! This module handles creating, dropping, and refreshing BM25 search indexes.

use pgrx::prelude::*;
use std::path::PathBuf;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter};

use crate::writer;

/// Build the Tantivy schema for our index
///
/// Two fields:
/// - pk: i64 - the table's primary key (for correlation)
/// - content: text - the searchable text field
fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    // Primary key field - INDEXED for deletion by term, STORED for retrieval
    builder.add_i64_field("pk", INDEXED | STORED);
    builder.add_text_field("content", TEXT);
    builder.build()
}

/// Get the path where the index is stored
pub fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    // Get PGDATA directory
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Primary key information for a table
#[derive(Clone, Debug)]
pub struct PrimaryKeyInfo {
    /// Name of the primary key column
    pub column_name: String,
    /// Data type of the primary key (e.g., "integer", "bigint")
    pub data_type: String,
}

/// Detect the primary key column for a table
///
/// Uses PostgreSQL system catalogs to find the primary key.
/// Returns None if the table has no primary key or has a composite PK.
pub fn get_primary_key_column(
    table_name: &str,
) -> Result<Option<PrimaryKeyInfo>, Box<dyn std::error::Error>> {
    let mut pk_info: Option<PrimaryKeyInfo> = None;

    Spi::connect(|client| {
        // Query to get primary key column name and type
        let query = format!(
            r#"
            SELECT a.attname::text, format_type(a.atttypid, a.atttypmod)::text
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid
                AND a.attnum = ANY(i.indkey)
            WHERE i.indrelid = '{}'::regclass
            AND i.indisprimary
        "#,
            table_name
        );

        // First check how many PK columns exist
        let count_result = client.select(
            &format!("SELECT COUNT(*) FROM ({}) t", query.trim()),
            None,
            &[],
        )?;

        let mut pk_count: i64 = 0;
        count_result.for_each(|row| {
            if let Ok(Some(c)) = row.get::<i64>(1) {
                pk_count = c;
            }
        });

        pgrx::info!(
            "pdb: Found {} primary key column(s) for table '{}'",
            pk_count,
            table_name
        );

        // Only proceed if exactly one PK column
        if pk_count == 1 {
            client.select(&query, None, &[])?.for_each(|row| {
                if let Ok(Some(col_name)) = row.get::<String>(1) {
                    if let Ok(Some(data_type)) = row.get::<String>(2) {
                        pgrx::info!(
                            "pdb: Detected PK column '{}' of type '{}'",
                            col_name,
                            data_type
                        );
                        pk_info = Some(PrimaryKeyInfo {
                            column_name: col_name,
                            data_type,
                        });
                    }
                }
            });
        }

        Ok::<(), spi::Error>(())
    })?;

    Ok(pk_info)
}

/// Ensures the metadata table exists for tracking indexed columns
///
/// The metadata table now includes primary key information for each indexed column.
fn ensure_metadata_table() -> Result<(), Box<dyn std::error::Error>> {
    Spi::run(
        "
        CREATE TABLE IF NOT EXISTS pdb_index_metadata (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            pk_column TEXT NOT NULL,
            pk_type TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW(),
            PRIMARY KEY (table_name, column_name)
        )
    ",
    )?;

    // Migration: Add new columns if they don't exist (for existing installations)
    let _ = Spi::run("ALTER TABLE pdb_index_metadata ADD COLUMN IF NOT EXISTS pk_column TEXT");
    let _ = Spi::run("ALTER TABLE pdb_index_metadata ADD COLUMN IF NOT EXISTS pk_type TEXT");

    Ok(())
}

/// Register an index in the metadata table
fn register_index_metadata(
    table_name: &str,
    column_name: &str,
    pk_info: &PrimaryKeyInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_metadata_table()?;

    Spi::run(&format!(
        "INSERT INTO pdb_index_metadata (table_name, column_name, pk_column, pk_type) 
         VALUES ('{}', '{}', '{}', '{}')",
        table_name, column_name, pk_info.column_name, pk_info.data_type
    ))?;

    Ok(())
}

/// Get primary key info for an indexed column from metadata
pub fn get_pk_info_for_index(
    table_name: &str,
    column_name: &str,
) -> Result<Option<PrimaryKeyInfo>, Box<dyn std::error::Error>> {
    let mut pk_info: Option<PrimaryKeyInfo> = None;

    Spi::connect(|client| {
        let query = format!(
            "SELECT pk_column, pk_type FROM pdb_index_metadata WHERE table_name = '{}' AND column_name = '{}'",
            table_name, column_name
        );

        client.select(&query, None, &[])?.for_each(|row| {
            if let (Ok(Some(pk_col)), Ok(Some(pk_type))) =
                (row.get::<String>(1), row.get::<String>(2))
            {
                pk_info = Some(PrimaryKeyInfo {
                    column_name: pk_col,
                    data_type: pk_type,
                });
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    Ok(pk_info)
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
/// 1. Detects the table's primary key
/// 2. Creates a Tantivy index on disk
/// 3. Scans all rows from the table via SPI
/// 4. Adds each row to the index with its PK
/// 5. Commits the index
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
///
/// # Requirements:
/// The table MUST have a single-column integer primary key (integer, bigint, or serial).
#[pg_extern]
pub fn create_bm25_index(
    table_name: &str,
    column_name: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Detect primary key
    let pk_info = get_primary_key_column(table_name)?
        .ok_or_else(|| format!(
            "Table '{}' must have a single-column primary key. Composite keys and tables without PKs are not supported.",
            table_name
        ))?;

    // Validate PK type (must be convertible to i64)
    let pk_type_lower = pk_info.data_type.to_lowercase();
    if !pk_type_lower.contains("int") && !pk_type_lower.contains("serial") {
        return Err(format!(
            "Primary key type '{}' is not supported. Only integer types (integer, bigint, serial) are supported.",
            pk_info.data_type
        ).into());
    }

    pgrx::info!(
        "Detected primary key: {} ({})",
        pk_info.column_name,
        pk_info.data_type
    );

    // 2. Build schema
    let schema = build_schema();
    let pk_field = schema.get_field("pk").expect("pk field missing");
    let content_field = schema.get_field("content").expect("content field missing");

    // 3. Create index directory
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

    // 4. Create Tantivy index
    let index = Index::create_in_dir(&index_path, schema)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?; // 50MB buffer

    // 5. Scan table and add documents
    let indexed_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let count_clone = indexed_count.clone();
    let pk_column = pk_info.column_name.clone();

    Spi::connect(|client| {
        // Select primary key and content column
        let query = format!(
            "SELECT {}::bigint, {} FROM {}",
            pk_column, column_name, table_name
        );

        client.select(&query, None, &[])?.for_each(|row| {
            // Get primary key as i64
            if let Ok(Some(pk)) = row.get::<i64>(1) {
                // Get text content
                if let Ok(Some(txt)) = row.get::<String>(2) {
                    let _ = writer.add_document(doc!(
                        pk_field => pk,
                        content_field => txt
                    ));
                    count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });

        Ok::<(), spi::Error>(())
    })?;

    // 6. Commit
    writer.commit()?;

    let count = indexed_count.load(std::sync::atomic::Ordering::Relaxed);

    // 7. Register in metadata table and create trigger
    register_index_metadata(table_name, column_name, &pk_info)?;
    create_sync_trigger(table_name, column_name)?;

    pgrx::info!(
        "✓ Created BM25 index on {}.{} ({} documents) with automatic sync via {}",
        table_name,
        column_name,
        count,
        pk_info.column_name
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

        // 3. Remove writer from cache
        writer::remove_writer(table_name, column_name);

        // 4. Remove index files
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
