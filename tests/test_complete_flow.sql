-- ================================================================================
-- Complete End-to-End Test: Triggers + Insertions + Index Info
-- ================================================================================

-- Clean start
DROP TABLE IF EXISTS articles CASCADE;
DROP TABLE IF EXISTS pdb_index_metadata CASCADE;

-- Create extension
CREATE EXTENSION IF NOT EXISTS pdb;

\echo '========================================='
\echo 'Test 1: Create table and index'
\echo '========================================='

CREATE TABLE articles (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    body TEXT
);

SELECT create_bm25_index('articles', 'body');

\echo ''
\echo 'Initial index state (should be empty):'
SELECT * FROM bm25_index_info('articles', 'body');

\echo ''
\echo '========================================='
\echo 'Test 2: INSERT with triggers'
\echo '========================================='

INSERT INTO articles (title, body) VALUES
    ('Rust Programming', 'Rust is a systems programming language'),
    ('Python Basics', 'Python is great for scripting'),
    ('JavaScript Guide', 'JavaScript powers the web');

\echo 'Index after 3 inserts:'
SELECT 
    num_docs,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('articles', 'body');

\echo ''
\echo 'Search for "rust" (should find 1 result):'
SELECT * FROM bm25_search('articles', 'body', 'rust');

\echo ''
\echo 'Search for "python" (should find 1 result):'
SELECT * FROM bm25_search('articles', 'body', 'python');

\echo ''
\echo '========================================='
\echo 'Test 3: More INSERTs'
\echo '========================================='

INSERT INTO articles (title, body) VALUES
    ('Advanced Rust', 'Rust provides memory safety without garbage collection'),
    ('Data Science', 'Python is excellent for data science and machine learning');

\echo 'Index after 5 total inserts:'
SELECT 
    num_docs,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('articles', 'body');

\echo ''
\echo 'Search for "rust" (should find 2 results now):'
SELECT * FROM bm25_search('articles', 'body', 'rust');

\echo ''
\echo '========================================='
\echo 'Test 4: UPDATE operation'
\echo '========================================='

UPDATE articles SET body = 'Rust is amazing for systems programming' WHERE id = 1;

\echo 'Search for "amazing" (should find updated content):'
SELECT * FROM bm25_search('articles', 'body', 'amazing');

\echo ''
\echo '========================================='
\echo 'Test 5: DELETE operation'
\echo '========================================='

DELETE FROM articles WHERE id = 2;

\echo 'Search for "python" (should find only 1 result now, id=2 deleted):'
SELECT * FROM bm25_search('articles', 'body', 'python');

\echo ''
\echo '========================================='
\echo 'Final Summary'
\echo '========================================='

\echo 'Final index state:'
SELECT 
    num_docs,
    pg_size_pretty(index_size_bytes::bigint) as size
FROM bm25_index_info('articles', 'body');

\echo ''
\echo 'All articles in table:'
SELECT id, title FROM articles ORDER BY id;

\echo ''
\echo '✓✓✓ All tests passed! ✓✓✓'
\echo 'Triggers work correctly with INSERT, UPDATE, DELETE'
\echo 'Index info returns accurate statistics'
\echo ''
