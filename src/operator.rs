//! # Operator Module - Inline Search Helpers
//!
//! Provides `pdb_match()`, `pdb_score()` for inline search in WHERE clauses,
//! and the `@@@` operator for natural search syntax.
//!
//! ## Usage
//! ```sql
//! -- Using functions (recommended):
//! SELECT * FROM articles
//! WHERE pdb_match('articles', 'body', 'rust programming', id::bigint)
//! ORDER BY pdb_score('articles', 'body', 'rust programming', id::bigint) DESC;
//!
//! -- Using @@@ operator (experimental):
//! SELECT pdb_search_init('articles', 'body', 'rust programming');
//! SELECT * FROM articles WHERE body @@@ 'any'
//! ORDER BY pdb_operator_score(id::bigint) DESC;
//! ```

use pgrx::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tantivy::{collector::TopDocs, query::QueryParser, Index};

/// Cached search results for the current session (for pdb_match/pdb_score)
#[derive(Clone, Debug, Default)]
struct SearchCache {
    /// Key: (table, column, query) -> Set of matching PKs
    matches: HashMap<(String, String, String), HashSet<i64>>,
    /// Key: (table, column, query, pk) -> Score
    scores: HashMap<(String, String, String, i64), f32>,
}

/// Search context for the @@@ operator
#[derive(Clone, Debug)]
struct SearchContext {
    table_name: String,
    column_name: String,
    query: String,
    matching_pks: HashSet<i64>,
    scores: HashMap<i64, f32>,
}

thread_local! {
    static CACHE: RefCell<SearchCache> = RefCell::new(SearchCache::default());
    static SEARCH_CTX: RefCell<Option<SearchContext>> = RefCell::new(None);
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
/// WHERE pdb_match('articles', 'body', 'rust programming', id::bigint);
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
/// SELECT id, title, pdb_score('articles', 'body', 'rust', id::bigint) as score
/// FROM articles
/// WHERE pdb_match('articles', 'body', 'rust', id::bigint)
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
/// This clears both the function cache and operator context.
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
    SEARCH_CTX.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
    true
}

/// Initialize search context for using the @@@ operator.
///
/// This sets up the search context that the @@@ operator and
/// `pdb_operator_score()` function use.
///
/// # SQL Usage:
/// ```sql
/// SELECT pdb_search_init('articles', 'body', 'rust programming');
/// SELECT * FROM articles WHERE body @@@ 'ignored'
/// ORDER BY pdb_operator_score(id::bigint) DESC;
/// ```
///
/// **Note**: The query parameter in the @@@ operator is currently ignored.
/// The query from `pdb_search_init()` is used instead.
///
/// # Arguments:
/// * `table_name` - Name of the table
/// * `column_name` - Name of the indexed column
/// * `query` - Search query string
#[pg_extern]
pub fn pdb_search_init(table_name: &str, column_name: &str, query: &str) -> bool {
    let index_path = get_index_path(table_name, column_name);
    let results = execute_tantivy_search(&index_path, query);

    let mut matching_pks = HashSet::new();
    let mut scores = HashMap::new();

    for (pk, score) in results {
        matching_pks.insert(pk);
        scores.insert(pk, score);
    }

    SEARCH_CTX.with(|ctx| {
        *ctx.borrow_mut() = Some(SearchContext {
            table_name: table_name.to_string(),
            column_name: column_name.to_string(),
            query: query.to_string(),
            matching_pks,
            scores,
        });
    });

    true
}

/// The @@@ operator for inline full-text search.
///
/// This operator provides a natural SQL syntax for search:
/// `WHERE column @@@ 'query'`
///
/// **IMPORTANT**: You must call `pdb_search_init()` FIRST to set up
/// the search context. The operator uses that context, not its parameters.
///
/// # SQL Usage:
/// ```sql
/// -- Step 1: Initialize (this does the actual search)
/// SELECT pdb_search_init('articles', 'body', 'rust programming');
///
/// -- Step 2: Use operator (filters based on initialized context)
/// SELECT * FROM articles
/// WHERE body @@@ 'ignored'  -- any string works, context is from init
/// ORDER BY pdb_operator_score(id::bigint) DESC;
/// ```
///
/// # Limitation:
/// The operator receives column **content**, not the row's primary key.
/// This makes it impossible to directly check if a row matches.
///
/// Currently, this operator returns `true` for all rows that have
/// the indexed column. Use `pdb_operator_score()` in ORDER BY to
/// rank results - rows with score > 0 matched the search.
///
/// # Arguments:
/// * `_content` - The column content (currently unused)
/// * `_query` - The search query (currently unused, uses pdb_search_init query)
///
/// # Returns:
/// * `true` if content is not null/empty
#[pg_operator(immutable, parallel_safe)]
#[opname(@@@)]
pub fn search_operator(_content: Option<&str>, _query: &str) -> bool {
    // Without access to the PK, we can't check if this row matches
    // Return true for non-null content, filtering happens via score
    _content.map_or(false, |c| !c.is_empty())
}

/// Get the BM25 score for a primary key (for use with @@@ operator).
///
/// This is similar to `pdb_score()` but works with the operator context
/// set by `pdb_search_init()`.
///
/// # SQL Usage:
/// ```sql
/// SELECT id, title, pdb_operator_score(id::bigint) as score
/// FROM articles
/// WHERE body @@@ 'ignored'
/// ORDER BY score DESC;
/// ```
///
/// Rows with score > 0 matched the search query.
/// Rows with score = 0 did not match.
///
/// # Arguments:
/// * `pk` - Primary key of the current row
#[pg_extern(immutable)]
pub fn pdb_operator_score(pk: i64) -> f32 {
    SEARCH_CTX.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .and_then(|c| c.scores.get(&pk).copied())
            .unwrap_or(0.0)
    })
}
