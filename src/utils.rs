//! # Utility Module
//!
//! This module contains utility functions for inspecting and managing indexes.

use pgrx::prelude::*;
use std::path::{Path, PathBuf};
use tantivy::Index;

/// Get the path where the index is stored
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Calculate total size of a directory recursively
fn calculate_dir_size(path: &Path) -> Result<i64, Box<dyn std::error::Error>> {
    let mut total_size = 0i64;

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_file() {
            total_size += metadata.len() as i64;
        } else if metadata.is_dir() {
            total_size += calculate_dir_size(&entry.path())?;
        }
    }

    Ok(total_size)
}

/// Returns information about an existing BM25 index.
///
/// # SQL Usage:
/// ```sql
/// SELECT * FROM bm25_index_info('articles', 'body');
/// --  num_docs | index_size_bytes
/// -- ----------|------------------
/// --    50000  | 15728640
/// ```
///
/// # Returns:
/// * `num_docs` - Number of documents in the index
/// * `index_size_bytes` - Total size of index on disk in bytes
///
/// # Use Cases:
/// - Troubleshooting: Check index size before expensive operations
/// - Monitoring: Track index growth over time
/// - Operations: Decide when to refresh based on metrics
/// - Health: Verify index creation succeeded
///
/// # Examples:
/// ```sql
/// -- Human-readable size
/// SELECT
///     num_docs,
///     pg_size_pretty(index_size_bytes::bigint) as size
/// FROM bm25_index_info('articles', 'body');
///
/// -- Monitor all indexes
/// SELECT
///     m.table_name,
///     m.column_name,
///     i.num_docs,
///     pg_size_pretty(i.index_size_bytes::bigint) as size
/// FROM pdb_index_metadata m
/// CROSS JOIN LATERAL bm25_index_info(m.table_name, m.column_name) i;
/// ```
#[pg_extern]
pub fn bm25_index_info(
    table_name: &str,
    column_name: &str,
) -> Result<
    TableIterator<'static, (name!(num_docs, i64), name!(index_size_bytes, i64))>,
    Box<dyn std::error::Error>,
> {
    // 1. Check if index exists
    let index_path = get_index_path(table_name, column_name);
    if !index_path.exists() {
        return Err(format!(
            "Index not found for {}.{}. Create it with create_bm25_index().",
            table_name, column_name
        )
        .into());
    }

    // 2. Open index
    let index = Index::open_in_dir(&index_path)?;

    // 3. Get reader and searcher
    let reader = index.reader()?;
    let searcher = reader.searcher();

    // 4. Get document count
    let num_docs = searcher.num_docs() as i64;

    // 5. Get index size on disk
    let index_size_bytes = calculate_dir_size(&index_path)?;

    // 6. Return as table
    let result = vec![(num_docs, index_size_bytes)];
    Ok(TableIterator::new(result.into_iter()))
}
