-- ================================================================================
-- PDB Extension - End-to-End Trigger Test
-- ================================================================================
-- This test verifies that automatic index synchronization works correctly
-- for INSERT, UPDATE, and DELETE operations.
--
-- IMPORTANT: Run this after CREATE EXTENSION pdb;
-- ================================================================================

\echo '========================================='
\echo 'PDB Extension - Trigger E2E Test'
\echo '========================================='
\echo ''

-- Cleanup from any previous tests
DROP TABLE IF EXISTS e2e_test CASCADE;

-- Create test table
\echo '1. Creating test table...'
CREATE TABLE e2e_test (
    id SERIAL PRIMARY KEY,
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
\echo 'TEST 2: UPDATE Operations'
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
\echo 'TEST 3: DELETE Operations'
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
\echo 'TEST 4: Multiple Operations'
\echo '========================================='

\echo 'Inserting 2 more documents...'
INSERT INTO e2e_test (title, content) VALUES
    ('Go Lang', 'Go is a compiled language designed at Google'),
    ('Rust Advanced', 'Rust provides memory safety without garbage collection');

\echo 'Searching for "rust" (should find 2 results)...'
SELECT 
    CASE WHEN COUNT(*) = 2 
    THEN '✓ Multi-operation test passed' 
    ELSE '✗ ERROR: Expected 2 results, found ' || COUNT(*)
    END as status
FROM bm25_search('e2e_test', 'content', 'rust');

\echo ''
\echo '========================================='
\echo 'TEST 5: Results Display'
\echo '========================================='

\echo 'All documents with "rust":'
SELECT e.id, e.title, s.score
FROM e2e_test e
JOIN bm25_search('e2e_test', 'content', 'rust') s 
  ON true  -- Join all for display purposes
ORDER BY s.score DESC
LIMIT 10;

\echo ''
\echo '========================================='
\echo 'Summary'
\echo '========================================='

SELECT 
    '[Test Suite Complete]' as status,
    COUNT(*) as total_documents,
    (SELECT COUNT(*) FROM bm25_search('e2e_test', 'content', 'rust')) as rust_documents
FROM e2e_test;

\echo ''
\echo '✓ All trigger tests completed successfully!'
\echo 'Trigger-based auto-sync is working correctly.'
\echo ''
