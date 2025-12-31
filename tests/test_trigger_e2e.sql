-- ================================================================================
-- PDB Extension - End-to-End Trigger Test (v2 - Transaction Safe)
-- ================================================================================
-- This test verifies:
-- 1. Automatic index synchronization for INSERT, UPDATE, DELETE
-- 2. Transaction safety (ROLLBACK discards index changes)
-- 3. Primary key correlation (JOINs work correctly)
--
-- IMPORTANT: Run this after CREATE EXTENSION pdb;
-- ================================================================================

\echo '========================================='
\echo 'PDB Extension - Trigger E2E Test v2'
\echo '========================================='
\echo ''

-- Cleanup from any previous tests
DROP TABLE IF EXISTS e2e_test CASCADE;

-- Create test table (using BIGSERIAL for i64 compatibility)
\echo '1. Creating test table...'
CREATE TABLE e2e_test (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    content TEXT
);

-- Create index with automatic trigger
\echo '2. Creating BM25 index (with trigger)...'
SELECT create_bm25_index('e2e_test', 'content');

-- Verify trigger was created
\echo '3. Verifying trigger exists...'
SELECT 
    CASE WHEN COUNT(*) > 0 
    THEN '✓ Trigger created successfully' 
    ELSE '✗ ERROR: Trigger not found!' 
    END as status
FROM pg_trigger 
WHERE tgrelid = 'e2e_test'::regclass 
AND tgname LIKE 'pdb_sync%';

\echo ''
\echo '========================================='
\echo 'TEST 1: INSERT Operations'
\echo '========================================='

-- Test INSERT - should auto-add to index
\echo 'Inserting 3 documents...'
INSERT INTO e2e_test (title, content) VALUES
    ('Rust Guide', 'Rust is a systems programming language focused on safety'),
    ('Python Tutorial', 'Python is great for data science and scripting'),
    ('JavaScript Basics', 'JavaScript runs in browsers and Node.js servers');

-- Immediately search - should find documents
\echo 'Searching for "rust" (should find 1 result)...'
SELECT 
    CASE WHEN COUNT(*) = 1 
    THEN '✓ INSERT test passed' 
    ELSE '✗ ERROR: Expected 1 result, found ' || COUNT(*)
    END as status
FROM bm25_search('e2e_test', 'content', 'rust');

\echo ''
\echo '========================================='
\echo 'TEST 2: JOIN with Primary Key'
\echo '========================================='

-- Test that pk column in search results matches the actual table PK
\echo 'Testing JOIN on pk column...'
SELECT 
    e.id as table_id, 
    s.pk as search_pk,
    e.title,
    round(s.score::numeric, 4) as score,
    CASE WHEN e.id = s.pk THEN '✓' ELSE '✗' END as pk_match
FROM bm25_search('e2e_test', 'content', 'rust') s
JOIN e2e_test e ON e.id = s.pk;

\echo ''
\echo '========================================='
\echo 'TEST 3: UPDATE Operations'
\echo '========================================='

-- Update a document
\echo 'Updating document 2 to mention Rust...'
UPDATE e2e_test 
SET content = 'Python and Rust are both excellent programming languages'
WHERE id = 2;

-- Search for rust - should now find 2 documents
\echo 'Searching for "rust" (should find 2 results now)...'
SELECT 
    CASE WHEN COUNT(*) = 2 
    THEN '✓ UPDATE test passed' 
    ELSE '✗ ERROR: Expected 2 results, found ' || COUNT(*)
    END as status
FROM bm25_search('e2e_test', 'content', 'rust');

-- Search for "data science" - should find 0 (was updated away)
\echo 'Searching for "data science" (should find 0 after update)...'
SELECT 
    CASE WHEN COUNT(*) = 0 
    THEN '✓ Old content removed from index' 
    ELSE '✗ ERROR: Old content still in index!'
    END as status
FROM bm25_search('e2e_test', 'content', 'data science');

\echo ''
\echo '========================================='
\echo 'TEST 4: DELETE Operations'
\echo '========================================='

-- Delete a document
\echo 'Deleting document 1...'
DELETE FROM e2e_test WHERE id = 1;

-- Search for rust - should now find only 1
\echo 'Searching for "rust" (should find 1 result after delete)...'
SELECT 
    CASE WHEN COUNT(*) = 1 
    THEN '✓ DELETE test passed' 
    ELSE '✗ ERROR: Expected 1 result, found ' || COUNT(*)
    END as status
FROM bm25_search('e2e_test', 'content', 'rust');

\echo ''
\echo '========================================='
\echo 'TEST 5: Transaction ROLLBACK'
\echo '========================================='

-- Test that ROLLBACK discards index changes
\echo 'Starting transaction and inserting a document...'
BEGIN;
INSERT INTO e2e_test (title, content) VALUES
    ('Should Not Exist', 'This rollback document should never be searchable');
\echo 'Rolling back transaction...'
ROLLBACK;

-- Search for the rolled-back content - should NOT find it
\echo 'Searching for "rollback" (should find 0 - transaction was rolled back)...'
SELECT 
    CASE WHEN COUNT(*) = 0 
    THEN '✓ ROLLBACK test passed - index changes were discarded' 
    ELSE '✗ ERROR: Found rolled-back content in index! Transaction safety broken!'
    END as status
FROM bm25_search('e2e_test', 'content', 'rollback');

\echo ''
\echo '========================================='
\echo 'TEST 6: Transaction COMMIT'
\echo '========================================='

-- Test that COMMIT persists index changes
\echo 'Starting transaction and inserting a document...'
BEGIN;
INSERT INTO e2e_test (title, content) VALUES
    ('Commit Test', 'This committed document should be searchable after commit');
\echo 'Committing transaction...'
COMMIT;

-- Search for the committed content - should find it
\echo 'Searching for "committed" (should find 1 - transaction was committed)...'
SELECT 
    CASE WHEN COUNT(*) = 1 
    THEN '✓ COMMIT test passed - index changes were persisted' 
    ELSE '✗ ERROR: Committed content not found in index!'
    END as status
FROM bm25_search('e2e_test', 'content', 'committed');

\echo ''
\echo '========================================='
\echo 'TEST 7: Multi-Statement Transaction'
\echo '========================================='

\echo 'Complex transaction with multiple operations...'
BEGIN;
INSERT INTO e2e_test (title, content) VALUES
    ('Multi 1', 'Transaction batch insert first document');
INSERT INTO e2e_test (title, content) VALUES
    ('Multi 2', 'Transaction batch insert second document');
UPDATE e2e_test SET content = 'Transaction updated to batch mode' WHERE title = 'Commit Test';
COMMIT;

\echo 'Searching for "batch" (should find 3 results)...'
SELECT 
    CASE WHEN COUNT(*) = 3 
    THEN '✓ Multi-statement transaction test passed' 
    ELSE '✗ ERROR: Expected 3 results, found ' || COUNT(*)
    END as status
FROM bm25_search('e2e_test', 'content', 'batch');

\echo ''
\echo '========================================='
\echo 'Summary'
\echo '========================================='

SELECT 
    '[Test Suite Complete]' as status,
    COUNT(*) as total_documents,
    (SELECT COUNT(*) FROM bm25_search('e2e_test', 'content', 'rust')) as rust_documents,
    (SELECT COUNT(*) FROM bm25_search('e2e_test', 'content', 'batch')) as batch_documents
FROM e2e_test;

\echo ''
\echo '✓ All trigger tests completed!'
\echo ''
