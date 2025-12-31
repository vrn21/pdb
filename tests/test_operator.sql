-- Test suite for @@@ operator

-- Setup test data
CREATE TABLE IF NOT EXISTS test_operator (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    body TEXT
);

TRUNCATE test_operator;

INSERT INTO test_operator (title, body) VALUES
  ('Rust Programming', 'Rust is a systems programming language with memory safety'),
  ('Python Guide', 'Python is a high-level programming language for beginners'),
  ('JavaScript Basics', 'JavaScript is the language of the web browser'),
  ('Learning Rust', 'Getting started with Rust for safe systems programming');

-- Create index
SELECT drop_bm25_index('test_operator', 'body');
SELECT create_bm25_index('test_operator', 'body');

-- Test 1: Initialize and use @@@ operator
SELECT pdb_search_init('test_operator', 'body', 'rust programming');

SELECT id, title, pdb_operator_score(id) as score
FROM test_operator
WHERE body @@@ 'ignored'  -- parameter is ignored, uses context from init
ORDER BY score DESC;
-- Expected: Rows with "rust" and "programming" ranked highest

-- Test 2: Filter by score > 0 to get only matches
SELECT id, title, pdb_operator_score(id) as score
FROM test_operator
WHERE body @@@ 'ignored' AND pdb_operator_score(id) > 0
ORDER BY score DESC;
-- Expected: Only rows that matched the search

-- Test 3: Change search query
SELECT pdb_search_init('test_operator', 'body', 'python');

SELECT id, title, pdb_operator_score(id) as score
FROM test_operator
WHERE body @@@ 'any'
ORDER BY score DESC;
-- Expected: Python article ranked first

-- Test 4: Compare with pdb_match/pdb_score
SELECT 
  id, 
  title,
  pdb_match('test_operator', 'body', 'rust', id) as using_match,
  pdb_score('test_operator', 'body', 'rust', id) as using_score
FROM test_operator
ORDER BY using_score DESC;
-- Expected: Same results as operator-based approach

-- Cleanup  
SELECT drop_bm25_index('test_operator', 'body');
DROP TABLE test_operator;
