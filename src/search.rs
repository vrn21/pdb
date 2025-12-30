//! # Search Module
//!
//! This module handles searching existing BM25 indexes and returning ranked results.

use pgrx::iter::TableIterator;
use pgrx::prelude::*;
use std::path::PathBuf;
use tantivy::{collector::TopDocs, query::QueryParser, Index};

/// Get the path where the index is stored (reused from index module)
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Search a BM25 index with a query string and return ranked results.
///
/// Returns all matching documents (up to 100 by default).
///
/// # SQL Usage:
/// ```sql
/// SELECT * FROM bm25_search('articles', 'body', 'rust programming');
/// -- Returns: row_id | score
/// ```
///
/// # Query Syntax:
/// - Simple: `rust`
/// - Boolean: `rust AND fast`
/// - Phrase: `"rust programming"`
/// - Exclusion: `rust -python`
/// - Boost: `rust^2 python`
#[pg_extern]
pub fn bm25_search(
    table_name: &str,
    column_name: &str,
    query_text: &str,
) -> Result<
    TableIterator<'static, (name!(row_id, i64), name!(score, f32))>,
    Box<dyn std::error::Error>,
> {
    bm25_search_limit(table_name, column_name, query_text, 100)
}

/// Search a BM25 index with a query string and return top N ranked results.
///
/// # SQL Usage:
/// ```sql
/// SELECT * FROM bm25_search_limit('articles', 'body', 'rust AND fast', 10);
/// -- Returns top 10: row_id | score
/// ```
///
/// # Arguments:
/// * `table_name` - Name of the table
/// * `column_name` - Name of the indexed column
/// * `query_text` - Search query (supports AND, OR, phrases, etc.)
/// * `limit` - Maximum number of results to return
#[pg_extern]
pub fn bm25_search_limit(
    table_name: &str,
    column_name: &str,
    query_text: &str,
    limit: i32,
) -> Result<
    TableIterator<'static, (name!(row_id, i64), name!(score, f32))>,
    Box<dyn std::error::Error>,
> {
    // 1. Check if index exists
    let index_path = get_index_path(table_name, column_name);
    if !index_path.exists() {
        return Err(format!(
            "No index found for {}.{}. Create one with create_bm25_index().",
            table_name, column_name
        )
        .into());
    }

    // 2. Open index
    let index = Index::open_in_dir(&index_path)?;
    let schema = index.schema();
    let row_id_field = schema.get_field("row_id")?;
    let content_field = schema.get_field("content")?;

    // 3. Create searcher
    let reader = index.reader()?;
    let searcher = reader.searcher();

    // 4. Parse query
    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query(query_text)?;

    // 5. Search
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit as usize))?;

    // 6. Extract results
    let mut results = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;

        if let Some(row_id_value) = doc.get_first(row_id_field) {
            if let tantivy::schema::OwnedValue::U64(row_id_u64) = row_id_value {
                results.push((*row_id_u64 as i64, score));
            }
        }
    }

    pgrx::info!(
        "🔍 Found {} result(s) for query '{}' on {}.{}",
        results.len(),
        query_text,
        table_name,
        column_name
    );

    // 7. Return as table
    Ok(TableIterator::new(results.into_iter()))
}
