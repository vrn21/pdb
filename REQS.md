Project Requirements: PostgreSQL Search
Extension with Rust (pgrx) and Analytics
This project will implement a Postgres extension in Rust (using pgrx) that adds advanced full-text search capabilities (and optionally simple analytics) inside a Postgres database. It will mirror key ideas from ParadeDB: embedding a Tantivy-based inverted index inside Postgres to support BM25 ranking, fuzzy search, etc., without external services 1 2 . The extension should integrate with the Postgres storage and planner so that indexes update in real time (automatically on INSERT/UPDATE/DELETE) 3 . Containerization and infrastructure scripts (e.g. Docker/Kubernetes/Terraform) will be provided to deploy and test the solution.
Real-world benchmarks show that integrating ParadeDB’s extensions into Postgres dramatically boosts performance. For example, Postgres with the pg_analytics extension (using Parquet tables) can be up to 94× faster than vanilla Postgres and 8× faster than Elasticsearch on OLAP queries, and its pg_search full-text engine can achieve 3–5× higher throughput (and correspondingly lower latency) than Elasticsearch under load 4 5.
Technologies & Stack
• PostgreSQL Extension (pgrx + Rust): The core of the project is a Postgres extension written in Rust via the pgrx framework. This enables using safe, idiomatic Rust to write functions, index methods, and custom types in Postgres 6 . The extension will target a modern Postgres version (13+).
• Full-Text Search (Tantivy): Use the Tantivy crate to build an on-disk inverted index. Tantivy is a Rust- based Lucene-alternative search engine library 1 . We will embed Tantivy inside Postgres to index text columns, enabling BM25 relevance scoring, fuzzy matching, and multi-language tokenization (features ParadeDB’s pg_search provides 1 2 ).
• Analytics (Arrow/DataFusion) – Optional: For a simple analytics component, we may define a Parquet-based table access method and use Apache Arrow/DataFusion for vectorized query execution. ParadeDB’s pg_analytics shows how Arrow and DataFusion can turn Postgres into an OLAP engine 7 . At minimum, the project can demonstrate creating a USING parquet table and running a simple aggregation, illustrating how Arrow/DFA accelerate columnar scans 7 .
• Infrastructure (Docker/Kubernetes/Terraform): Provide deployment artifacts so the extension can be built, installed, and run in a realistic environment. This includes a Dockerfile for a Postgres image with the extension installed, Kubernetes YAML or Helm chart to run a Postgres pod, and Terraform scripts to provision any required resources (e.g. cloud VM or K8s cluster). Bash or Go scripts may automate building/testing. The goal is to mirror a real deployment pipeline (e.g. “deploy a Postgres cluster with this extension on Kubernetes”).
1
These technologies match ParadeDB’s stack: “built in Rust via Postgres extensions on top of database building blocks like Tantivy, DuckDB, and Apache DataFusion” 8 .
Functional Requirements
• Custom Index Type – BM25 Full-Text Index: Implement a new index access method (similar to an AM for GIN/BRIN) to store an inverted index for one or more text columns. This index will use Tantivy internally (the inverted index structure) so that each term points to matching row IDs 9 . The index must update automatically with table changes: “When a BM25 index is created, Postgres automatically updates it as new data arrives or is deleted in the underlying SQL table,” enabling real- time search without manual reindexing 3 . (We will hook into Postgres’s index AM API via pgrx so inserts/updates propagate to the Tantivy index.)
• SQL Query Interface: Provide a SQL-level way to run searches against the index. For example, define a custom operator (e.g. @@@ ) or function to invoke searches. ParadeDB’s design uses
table @@@ query syntax, leveraging the operator API 10 . We should choose a clear syntax (perhaps @@@ ) that accepts a query string in Tantivy’s mini-language or JSON configuration. The operator/function should take a query and return matching rows with relevance scores.
• Search Features – BM25 & Fuzzy: Support BM25 relevance scoring (as a ranking heuristic) and allow basic query syntax (boolean OR, field-specific search). Include fuzzy matching (approximate text matching) and language-aware tokenization by configuring Tantivy analyzers. Optionally support result highlighting or snippet extraction. These advanced features are core to pg_search : “fuzzy search, aggregations, highlighting, and relevance tuning,” all using BM25 scoring 11 .
• Transactional & ACID Compliance: Ensure the extension honors Postgres’s transactional model. Index updates (from inserts/deletes) should occur within the same transaction context or deferrable contexts. While full snapshot isolation may be complex, the project should at least not break transactional guarantees. (E.g. on transaction abort, index changes should be rolled back.)
• Schema and Configuration: Allow creating the index with a definition of which text columns to index (and any numeric fields for filtering). E.g.:
-- Create search index on table my_table (id, title, description)
CREATE INDEX ON my_table USING pg_bm25 (description) WITH (bm25_config =
'{ "fuzzy": true }');
We’ll need to parse configuration options (likely JSON) in Rust via pgrx macros. All index metadata (schema, options) should be stored in Postgres catalogs.
• Example Query: Verify with a test table. For instance, after creating an index, the user should run:
SELECT id, title
FROM my_table
WHERE my_table @@@ '"quick fox"~2' -- an example fuzzy search query
ORDER BY score DESC
LIMIT 10;
This should return rows ranked by BM25.
2

• Analytics Component (optional): Extend the extension (or a companion one) to demonstrate a simplecolumnarstoragefeature.Forexample,implementa CREATE TABLE ... USING parquet access method that stores data in Parquet/Arrow format. Use DataFusion’s libraries to execute basic SQL scans/aggregations. ParadeDB’s pg_analytics does this: “add column-oriented storage and vectorized query execution to Postgres” using Arrow/DataFusion 7 . We can include a minimal demo: create a Parquet table and run SELECT AVG(col) FROM table; and verify it executes using Arrow. This is optional but shows familiarity with their analytics building blocks.
Implementation Steps

1. Project Setup: Initialize a new Rust extension project with pgrx ( cargo pgrx new pg_search_demo ). Set up CI (unit tests) and ensure the development environment is ready (Postgres dev headers, etc.).
2. Index Access Method (Rust/C): Using pgrx, define a new index AM struct and callbacks. Follow examples (the pgrx examples repo or tutorials) to register an index opclass and AM. Ensure Postgres can create an index of our type.
3. Embed Tantivy: In the Rust code, link to the Tantivy crate. During index build or on-the-fly, create a Tantivy index (in a directory specific to this Postgres instance). For simplicity, you can use a single file directory per index. Write code so that on INSERT/UPDATE/DELETE, the new/old row is added/ removed in the Tantivy index. (This might use Postgres hooks via pgrx, or a background worker).
4. Inverted Index Logic: Build the inverted index by extracting text from rows. For each indexed row, add a Tantivy document with the text fields (and maybe numeric fields). Use a consistent doc ID (e.g. the table’s primary key). Investigate how to implement merge or deletes (Tantivy allows marking docs deleted). Ensure the index is persisted.
5. Query Interface: Register a PostgreSQL operator/function. Using #[pg_extern] , create a function like pg_search_query(index_name text, query_text text) RETURNS SETOF my_table . This function should parse the query string into a Tantivy query, run it, and yield matching rows. Or, implement a boolean operator ( @@@ ) via the operator API. Use pgrx’s SPI if needed to fetch rows by ID from the base table once you have search hits. The ParadeDB blog shows they map operator RHS into Tantivy’s mini language 10 .
6. BM25 Ranking & Options: Configure Tantivy to use BM25 (default). Expose tunable parameters (k1, b) if desired. Also add configuration to enable fuzzy search in queries. Confirm the extension uses BM25 for scoring (like Elasticsearch). Document any query syntax differences.
7. Real-Time Index Updates: Test that new rows become searchable immediately. According to ParadeDB, “new data is immediately searchable without manual reindexing” 12 . Verify by inserting a row and then querying. If using WAL or triggers is too complex, at minimum allow manual refresh or simulate via function call.
8. Testing with Example Data: Prepare a small dataset (e.g. a few thousand text records). Use provided example SQL (like in 2 ) to create a test table, insert data, create the index, and run sample queries. Confirm results and measure basic performance (even rough) versus vanilla
   to_tsvector if possible.
9. (Optional) Parquet/Analytics Demo: Implement a basic custom table AM that stores data in
   Parquet. Use Arrow/DataFusion to scan the table. This could reuse existing crates for Parquet I/O. At least demonstrate CREATE TABLE ... USING parquet; INSERT ...; SELECT ...; works. (Given one week, this might be a stretch; it can be simplified or scoped down, but it is part of the ParadeDB stack 7 .)
   3

10. Performance Benchmark: If time permits, run a simple benchmark (e.g. on a synthetic dataset) comparing query time vs Postgres built-in text search. Document the results briefly. For search, one might compare against tsvector @@ tsquery on the same data. For analytics, compare a GROUP BY on a Parquet table vs a normal table.
11. Infrastructure & Deployment:
    ◦ Docker: Write a Dockerfile that builds and installs the extension into a Postgres image
    (Postgres 14 or 15). The image should start postgres and allow SQL connections.
    ◦ Kubernetes: Provide a simple Kubernetes manifest (or Helm chart) that deploys the Postgres
    container. Include a PersistentVolume for storage if needed.
    ◦ Terraform: (Optional) Write Terraform scripts to spin up a test environment, e.g. a cloud VM
    or managed K8s cluster, and deploy the Helm chart there. This simulates real-world
    deployment.
    ◦ Scripts: Add a Bash or Go script to automate building the Docker image, loading it into a local
    cluster (e.g. kind/minikube), and running migrations (creating extension, tables).
12. Documentation: Write clear README or markdown docs. Include: project goals, how to build/install
    the extension, SQL examples (CREATE EXTENSION, CREATE INDEX, example queries), and how to deploy via the provided Docker/K8s/Terraform. Cite any relevant sources (as done here) and explain how the project maps to ParadeDB’s technology.
    Summary
    The result will be a working Postgres extension (in Rust) that adds Elasticsearch-like search inside Postgres, along with basic support for columnar analytics. It will demonstrate using pgrx for Postgres internals, Tantivy for an inverted index (enabling BM25 scoring and fuzzy search), and optionally Arrow/ DataFusion for analytics queries 8 7 . This mini-project mirrors ParadeDB’s approach of embedding search and analytics into Postgres via extensions 1 2 .
    By the end of the week, you will have a prototype extension and a deployment setup (Docker/K8s/ Terraform), showcasing how data systems internals and search/analytics engines can be combined in Rust. This gives practical experience with the core technologies in ParadeDB’s stack: Postgres internals, pgrx, Tantivy, and data analytics libraries 6 7 .
    Citations: References above show ParadeDB’s design and performance benchmarks 1 7 4 5 , and the use of Rust/pgrx for Postgres extensions 6 8 . Each requirement in this document is grounded in those real-world implementations.
    1 3 10 11 12 pg_search: Elastic-Quality Full Text Search Inside Postgres | ParadeDB https://www.paradedb.com/blog/introducing-search
    2 4 5 7 Starlet #20 ParadeDB: Postgres for Search & Analytics https://www.star-history.com/blog/paradedb
    6 Search on PostgreSQL, Building Extensions, and pg_analytics with Philippe Noël https://materializedview.io/p/search-on-postgresql-building-extensions
    4

8 [Building Blocks] ParadeDB - Postgres for Search and Analytics (Philippe Noël) - Carnegie Mellon Database Group
https://db.cs.cmu.edu/events/building-blocks-paradedb-philippe-noel/
9 tantivy/ARCHITECTURE.md at main · quickwit-oss/tantivy · GitHub https://github.com/quickwit-oss/tantivy/blob/main/ARCHITECTURE.md
