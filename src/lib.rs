//! # PDB - PostgreSQL Search Extension
//!
//! A Postgres extension that provides BM25 full-text search using Tantivy.
//! This extension allows you to create inverted indexes on text columns
//! and run relevance-ranked searches directly from SQL.
//!

/// Index management: create, drop, refresh indexes
pub mod index;

/// Search execution: query the index, get ranked results
pub mod search;

/// Trigger functions: keep index in sync with table changes
pub mod trigger;

/// Utility functions: index info, diagnostics
pub mod utils;

/// Transaction-aware writer: buffers operations until commit
pub mod writer;

// This macro registers the extension with PostgreSQL.
// It tells Postgres: "This is an extension called 'pdb' at version X"
::pgrx::pg_module_magic!();

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::prelude::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_create_index() {
        // This test will run inside a real Postgres instance
        let result = crate::index::create_bm25_index("test_table", "test_column");
        assert!(result.is_ok());
    }
}

/// Required by `cargo pgrx test`
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // One-time setup before tests run
    }

    #[must_use]
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // Any postgres.conf settings needed for tests
        vec![]
    }
}
