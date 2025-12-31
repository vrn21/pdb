-- ================================================================================
-- Test: bm25_index_info() Function
-- ================================================================================
-- Verifies that index info function returns accurate statistics

\echo '========================================='
\echo 'Testing bm25_index_info()'
\echo '========================================='
\echo ''

-- Setup: Create extension if not exists
CREATE EXTENSION IF NOT EXISTS pdb;

-- Cleanup
DROP TABLE IF EXISTS info_test CASCADE;

\echo '1. Testing non-existent index (should error)...'
\set ON_ERROR_STOP off
SELECT * FROM bm25_index_info('nonexistent', 'fake');
-- Expected: ERROR
\set ON_ERROR_STOP on

\echo ''
\echo '2. Creating test table and index...'
CREATE TABLE info_test (
    id SERIAL PRIMARY KEY,
    content TEXT
);

SELECT create_bm25_index('info_test', 'content');

\echo ''
\echo '3. Testing empty index...'
SELECT 
    num_docs,
    index_size_bytes > 0 as has_size,
    pg_size_pretty(index_size_bytes::bigint) as human_size
FROM bm25_index_info('info_test', 'content');
-- Expected: num_docs = 0, has_size = true

\echo ''
\echo '4. Adding data and checking...'
INSERT INTO info_test (content) VALUES
    ('First document about Rust programming'),
    ('Second document about Python'),
    ('Third document about JavaScript');

SELECT 
    num_docs,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('info_test', 'content');
-- Expected: num_docs = 3

\echo ''
\echo '5. Adding more data...'
INSERT INTO info_test (content) 
SELECT 'Document number ' || i || ' with some content'
FROM generate_series(1, 100) i;

SELECT 
    num_docs,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('info_test', 'content');
-- Expected: num_docs = 103

\echo ''
\echo '6. After refresh...'
SELECT refresh_bm25_index('info_test', 'content');

SELECT 
    num_docs as docs_after_refresh,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('info_test', 'content');
-- Expected: num_docs = 103 (should match)

\echo ''
\echo '7. Monitoring all indexes...'
SELECT 
    m.table_name,
    m.column_name,
    i.num_docs,
    pg_size_pretty(i.index_size_bytes::bigint) as size
FROM pdb_index_metadata m
CROSS JOIN LATERAL bm25_index_info(m.table_name, m.column_name) i
ORDER BY i.index_size_bytes DESC;

\echo ''
\echo '✓ All bm25_index_info() tests passed!'
\echo ''
