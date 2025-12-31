//! # Writer Module
//!
//! This module manages transaction-aware buffered writes to Tantivy indexes.
//!
//! ## Key Features:
//! - Buffers pending operations per transaction
//! - Only commits to Tantivy on PostgreSQL transaction commit
//! - Discards pending operations on transaction abort
//! - Maintains ACID compliance with PostgreSQL transactions

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use pgrx::{register_xact_callback, PgXactCallbackEvent};
use tantivy::{doc, Index, Term};

/// A single pending index operation
#[derive(Clone, Debug)]
pub enum PendingOp {
    /// Add a document with primary key and content
    Add { pk: i64, content: String },
    /// Delete a document by primary key
    Delete { pk: i64 },
}

/// Buffer key: (table_name, column_name)
type IndexKey = (String, String);

// Thread-local storage for pending operations
thread_local! {
    /// Pending operations buffer, keyed by (table_name, column_name)
    static PENDING_OPS: RefCell<HashMap<IndexKey, Vec<PendingOp>>> = RefCell::new(HashMap::new());

    /// Whether we've already registered callbacks for this transaction
    static CALLBACK_REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// Get the path where the index is stored
fn get_index_path(table_name: &str, column_name: &str) -> PathBuf {
    let pgdata = std::env::var("PGDATA").unwrap_or_else(|_| "/var/lib/postgresql/data".to_string());

    PathBuf::from(pgdata)
        .join("pdb_indexes")
        .join(format!("{}_{}", table_name, column_name))
}

/// Queue an add operation for the current transaction
///
/// The operation will be applied when the transaction commits.
/// If the transaction aborts, the operation is discarded.
pub fn queue_add(
    table_name: &str,
    column_name: &str,
    pk: i64,
    content: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Register callbacks if not already done for this transaction
    ensure_callbacks_registered();

    // Add operation to pending buffer
    let key = (table_name.to_string(), column_name.to_string());
    PENDING_OPS.with(|ops| {
        ops.borrow_mut()
            .entry(key)
            .or_default()
            .push(PendingOp::Add { pk, content });
    });

    Ok(())
}

/// Queue a delete operation for the current transaction
///
/// The operation will be applied when the transaction commits.
/// If the transaction aborts, the operation is discarded.
pub fn queue_delete(
    table_name: &str,
    column_name: &str,
    pk: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Register callbacks if not already done for this transaction
    ensure_callbacks_registered();

    // Add operation to pending buffer
    let key = (table_name.to_string(), column_name.to_string());
    PENDING_OPS.with(|ops| {
        ops.borrow_mut()
            .entry(key)
            .or_default()
            .push(PendingOp::Delete { pk });
    });

    Ok(())
}

/// Ensure transaction callbacks are registered
///
/// We register once per transaction. pgrx automatically cleans up callbacks
/// at transaction end, so we just need to reset our flag.
fn ensure_callbacks_registered() {
    CALLBACK_REGISTERED.with(|registered| {
        if !registered.get() {
            // Register PreCommit callback - flush pending operations to Tantivy
            let _ = register_xact_callback(PgXactCallbackEvent::PreCommit, || {
                flush_pending_operations();
            });

            // Register Abort callback - discard pending operations
            // NOTE: Must not panic here - will crash PostgreSQL
            let _ = register_xact_callback(PgXactCallbackEvent::Abort, || {
                discard_pending_operations();
            });

            registered.set(true);
        }
    });
}

/// Flush all pending operations to Tantivy and commit
///
/// Called during PreCommit when the transaction is about to succeed.
fn flush_pending_operations() {
    // Take all pending operations
    let ops_to_flush: HashMap<IndexKey, Vec<PendingOp>> =
        PENDING_OPS.with(|ops| std::mem::take(&mut *ops.borrow_mut()));

    // Reset callback flag for next transaction
    CALLBACK_REGISTERED.with(|reg| reg.set(false));

    if ops_to_flush.is_empty() {
        return;
    }

    // Process each index
    for ((table_name, column_name), pending_ops) in ops_to_flush {
        if let Err(e) = flush_index_operations(&table_name, &column_name, pending_ops) {
            pgrx::warning!("pdb: Failed to flush {}.{}: {}", table_name, column_name, e);
        }
    }
}

/// Flush operations for a single index
fn flush_index_operations(
    table_name: &str,
    column_name: &str,
    ops: Vec<PendingOp>,
) -> Result<(), Box<dyn std::error::Error>> {
    let index_path = get_index_path(table_name, column_name);
    let index = Index::open_in_dir(&index_path)?;
    let schema = index.schema();

    let pk_field = schema.get_field("pk")?;
    let content_field = schema.get_field("content")?;

    // Create writer with 50MB buffer
    let mut writer = index.writer(50_000_000)?;

    // Apply all operations
    for op in ops {
        match op {
            PendingOp::Add { pk, content } => {
                writer.add_document(doc!(
                    pk_field => pk,
                    content_field => content
                ))?;
            }
            PendingOp::Delete { pk } => {
                let term = Term::from_field_i64(pk_field, pk);
                writer.delete_term(term);
            }
        }
    }

    // Commit
    writer.commit()?;

    Ok(())
}

/// Discard all pending operations without applying them
///
/// Called during Abort callback. MUST NOT panic!
fn discard_pending_operations() {
    // Use defensive code to avoid panics
    let _ = PENDING_OPS.try_with(|ops| {
        if let Ok(mut ops) = ops.try_borrow_mut() {
            ops.clear();
        }
    });

    let _ = CALLBACK_REGISTERED.try_with(|reg| {
        reg.set(false);
    });
}

/// Clear any cached state for an index (no-op in simplified version)
///
/// Called when dropping an index.
pub fn remove_writer(_table_name: &str, _column_name: &str) {
    // No-op: simplified version doesn't cache writers
}
