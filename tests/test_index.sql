-- Test script for index module
-- This will be run in psql after loading the extension

-- 1. Create extension
CREATE EXTENSION IF NOT EXISTS pdb;

-- 2. Create a test table
DROP TABLE IF EXISTS test_articles;
CREATE TABLE test_articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    body TEXT
);

-- 3. Insert test data
INSERT INTO test_articles (title, body) VALUES
    ('Rust Guide', 'Rust is a systems programming language that is blazingly fast and memory-safe.'),
    ('Python Tutorial', 'Python is great for scripting and data analysis.'),
    ('Rust vs Python', 'Comparing Rust and Python for performance and safety.'),
    ('Web Development', 'Building web applications with modern frameworks.'),
    ('Database Systems', 'Understanding how databases work under the hood.');

-- 4. Test: Create index
\echo '==== Testing create_bm25_index ===='
SELECT create_bm25_index('test_articles', 'body');

-- 5. Verify index files were created
\! ls -la $PGDATA/pdb_indexes/test_articles_body/ 2>/dev/null || echo "Index directory not found"

-- 6. Test: Try creating the same index again (should fail or warn)
\echo '==== Testing duplicate index creation ===='
SELECT create_bm25_index('test_articles', 'body');

-- 7. Test: Drop index
\echo '==== Testing drop_bm25_index ===='
SELECT drop_bm25_index('test_articles', 'body');

-- 8. Verify index files were deleted
\! ls -la $PGDATA/pdb_indexes/test_articles_body/ 2>/dev/null || echo "Index directory removed ✓"

-- 9. Test: Drop non-existent index (should warn)
\echo '==== Testing drop of non-existent index ===='
SELECT drop_bm25_index('test_articles', 'body');

-- 10. Test: Refresh (recreate)
\echo '==== Testing refresh_bm25_index ===='
SELECT create_bm25_index('test_articles', 'body');
SELECT refresh_bm25_index('test_articles', 'body');

-- 11. Cleanup
DROP TABLE test_articles;
SELECT drop_bm25_index('test_articles', 'body');

\echo '==== All tests completed ===='
