-- Test suite for inline search functions (pdb_match and pdb_score)

-- Setup test data
CREATE TABLE IF NOT EXISTS test_articles (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    body TEXT
);

TRUNCATE test_articles;

INSERT INTO test_articles (title, body) VALUES
  ('Rust Programming', 'Rust is a systems programming language with memory safety'),
  ('Python Guide', 'Python is a high-level programming language for beginners'),
  ('JavaScript Basics', 'JavaScript is the language of the web browser'),
  ('Learning Rust', 'Getting started with Rust for safe systems programming');

-- Create index
SELECT create_bm25_index('test_articles', 'body');

-- Test 1: Basic pdb_match - should return true/false for each row
SELECT id, title, pdb_match('test_articles', 'body', 'rust', id::bigint) as matches 
FROM test_articles
ORDER BY id;
-- Expected: rows with 'rust' in body return true

-- Test 2: Basic pdb_score - should return scores for all rows
SELECT id, title, pdb_score('test_articles', 'body', 'rust', id::bigint) as score 
FROM test_articles
ORDER BY id;
-- Expected: rows with 'rust' have score > 0, others have score = 0

-- Test 3: WHERE clause filtering with pdb_match
SELECT id, title, pdb_score('test_articles', 'body', 'rust', id::bigint) as score
FROM test_articles
WHERE pdb_match('test_articles', 'body', 'rust', id::bigint)
ORDER BY score DESC;
-- Expected: only rows matching 'rust', ordered by relevance

-- Test 4: Combined filters (search + regular SQL WHERE)
SELECT id, title, pdb_score('test_articles', 'body', 'programming', id::bigint) as score
FROM test_articles
WHERE pdb_match('test_articles', 'body', 'programming', id::bigint)
  AND title LIKE '%Rust%'
ORDER BY score DESC;
-- Expected: only Rust articles that contain 'programming'

-- Test 5: Multi-word search query
SELECT id, title, pdb_score('test_articles', 'body', 'systems programming', id::bigint) as score
FROM test_articles
WHERE pdb_match('test_articles', 'body', 'systems programming', id::bigint)
ORDER BY score DESC;
-- Expected: articles with both 'systems' and 'programming' ranked higher

-- Test 6: Cache clearing
SELECT pdb_clear_cache();
-- Expected: returns true

-- Test 7: Search after cache clear (should work the same)
SELECT id, title, pdb_score('test_articles', 'body', 'rust', id::bigint) as score
FROM test_articles
WHERE pdb_match('test_articles', 'body', 'rust', id::bigint)
ORDER BY score DESC;
-- Expected: same results as Test 3

-- Cleanup
SELECT drop_bm25_index('test_articles', 'body');
DROP TABLE test_articles;
