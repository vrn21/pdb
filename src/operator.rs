//! # Operator Module - Inline Search Helpers
//!
//! Provides `pdb_match()` and `pdb_score()` for inline search in WHERE clauses.
//!
//! ## Usage
//! ```sql
//! SELECT * FROM articles
//! WHERE pdb_match('articles', 'body', 'rust programming', id)
//! ORDER BY pdb_score('articles', 'body', 'rust programming', id) DESC;
//! ```

use pgrx::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tantivy::{collector::TopDocs, query::QueryParser, Index};

/// Cached search results for the current session
#[derive(Clone, Debug, Default)]
struct SearchCache {
    /// Key: (table, column, query) -> Set of matching PKs
    matches: HashMap<(String, String, String), HashSet<i64>>,
    /// Key: (table, column, query, pk) -> Score
    scores: HashMap<(String, String, String, i64), f32>,
}

thread_local! {
    static CACHE: RefCell<SearchCache> = RefCell::new(SearchCache::default());
}

/// Get the path where the index is stored
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());
    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Execute search and cache results
fn ensure_search_cached(table_name: &str, column_name: &str, query_text: &str) {
    let cache_key = (
        table_name.to_string(),
        column_name.to_string(),
        query_text.to_string(),
    );

    // Check if already cached
    let is_cached = CACHE.with(|c| c.borrow().matches.contains_key(&cache_key));
    if is_cached {
        return;
    }

    // Execute search
    let index_path = get_index_path(table_name, column_name);
    let results = execute_tantivy_search(&index_path, query_text);

    // Cache results
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let mut pk_set = HashSet::new();

        for (pk, score) in results {
            pk_set.insert(pk);
            cache.scores.insert(
                (
                    table_name.to_string(),
                    column_name.to_string(),
                    query_text.to_string(),
                    pk,
                ),
                score,
            );
        }

        cache.matches.insert(cache_key, pk_set);
    });
}

/// Execute the actual Tantivy search
fn execute_tantivy_search(index_path: &PathBuf, query_text: &str) -> Vec<(i64, f32)> {
    let index = match Index::open_in_dir(index_path) {
        Ok(idx) => idx,
        Err(_) => return vec![],
    };

    let reader = match index.reader() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let searcher = reader.searcher();
    let schema = index.schema();

    let pk_field = match schema.get_field("pk") {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let content_field = match schema.get_field("content") {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let parser = QueryParser::for_index(&index, vec![content_field]);
    let query = match parser.parse_query(query_text) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    let top_docs = match searcher.search(&query, &TopDocs::with_limit(10000)) {
        Ok(docs) => docs,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();
    for (score, doc_addr) in top_docs {
        if let Ok(doc) = searcher.doc::<tantivy::TantivyDocument>(doc_addr) {
            if let Some(tantivy::schema::OwnedValue::I64(pk)) = doc.get_first(pk_field) {
                results.push((*pk, score));
            }
        }
    }

    results
}

/// Check if a row matches a search query.
///
/// This function enables inline filtering in WHERE clauses. It caches the
/// search results so repeated calls (for each row) are efficient.
///
/// # SQL Usage:
/// ```sql
/// SELECT * FROM articles
/// WHERE pdb_match('articles', 'body', 'rust programming', id);
/// ```
///
/// # Arguments:
/// * `table_name` - Name of the table
/// * `column_name` - Name of the indexed column
/// * `query` - Search query string
/// * `pk` - Primary key of the current row
#[pg_extern(immutable)]
pub fn pdb_match(table_name: &str, column_name: &str, query: &str, pk: i64) -> bool {
    ensure_search_cached(table_name, column_name, query);

    CACHE.with(|c| {
        c.borrow()
            .matches
            .get(&(
                table_name.to_string(),
                column_name.to_string(),
                query.to_string(),
            ))
            .map_or(false, |pks| pks.contains(&pk))
    })
}

/// Get the BM25 score for a specific row.
///
/// Returns 0.0 if the row doesn't match or isn't in the cache.
///
/// # SQL Usage:
/// ```sql
/// SELECT id, title, pdb_score('articles', 'body', 'rust', id) as score
/// FROM articles
/// WHERE pdb_match('articles', 'body', 'rust', id)
/// ORDER BY score DESC;
/// ```
///
/// # Arguments:
/// * `table_name` - Name of the table
/// * `column_name` - Name of the indexed column
/// * `query` - Search query string
/// * `pk` - Primary key of the current row
#[pg_extern(immutable)]
pub fn pdb_score(table_name: &str, column_name: &str, query: &str, pk: i64) -> f32 {
    ensure_search_cached(table_name, column_name, query);

    CACHE.with(|c| {
        c.borrow()
            .scores
            .get(&(
                table_name.to_string(),
                column_name.to_string(),
                query.to_string(),
                pk,
            ))
            .copied()
            .unwrap_or(0.0)
    })
}

/// Clear the search cache.
///
/// This is useful at the end of a transaction or when you want to force
/// a fresh search. Normally, the cache is cleared automatically when the
/// PostgreSQL backend process ends.
///
/// # SQL Usage:
/// ```sql
/// SELECT pdb_clear_cache();
/// ```
#[pg_extern]
pub fn pdb_clear_cache() -> bool {
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.matches.clear();
        cache.scores.clear();
    });
    true
}
