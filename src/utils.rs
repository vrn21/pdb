//! # Utility Module
//!
//! This module contains utility functions for inspecting and managing indexes.

use pgrx::prelude::*;

/// Returns information about an existing index.
///
/// # SQL Usage:
/// ```sql
/// SELECT * FROM bm25_index_info('articles', 'body');
/// ```
///
/// # Returns:
/// * num_docs - Number of documents in the index
/// * index_size_bytes - Size of index on disk
#[pg_extern]
pub fn bm25_index_info(
    table_name: &str,
    column_name: &str,
) -> TableIterator<'static, (name!(num_docs, i64), name!(index_size_bytes, i64))> {
    // TODO: Implementation will go here
    pgrx::info!("Would get info for {}.{}", table_name, column_name);
    TableIterator::new(std::iter::empty())
}
