# pdb - BM25 Full-Text Search for PostgreSQL

A PostgreSQL extension that provides **Elasticsearch-quality full-text search** using Tantivy (BM25) directly in your database.

## What It Does

- **`@@@` operator** - Natural SQL syntax for full-text search in WHERE clauses
- **BM25 ranking** - Relevance-ranked search results, not just pattern matching
- **Automatic sync** - Indexes update on INSERT/UPDATE/DELETE via triggers
- **Transaction safe** - ROLLBACK discards index changes, COMMIT persists them
- **Primary key correlation** - Search results JOIN directly to your tables

## Quick Start

```sql
-- Create extension
CREATE EXTENSION pdb;

-- Create a table with text you want to search
CREATE TABLE articles (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    body TEXT
);

-- Create a BM25 index on the body column
SELECT create_bm25_index('articles', 'body');

-- Insert some data (index updates automatically)
INSERT INTO articles (title, body) VALUES
    ('Rust Guide', 'Rust is a systems programming language'),
    ('Python Tutorial', 'Python is great for data science');

-- Search using @@@ operator
SELECT pdb_search_init('articles', 'body', 'rust programming');

SELECT id, title, pdb_operator_score(id) as score
FROM articles
WHERE body @@@ 'rust'
  AND pdb_operator_score(id) > 0  -- filter to actual matches
ORDER BY score DESC;
```

## Installation

### Local Development (pgrx)

```bash
# Install Rust + pgrx
cargo install cargo-pgrx --version 0.16.1
cargo pgrx init --pg17 $(which pg_config)

# Build and run
cargo pgrx run pg17

# In psql
CREATE EXTENSION pdb;
```

### Docker

```bash
# Build image
docker build -t pdb-postgres .

# Run container
docker compose up -d

# Connect
psql -h localhost -p 5432 -U pdb -d pdb_dev
```

### Kubernetes

```bash
# Setup local cluster (kind + kubectl)
./scripts/k8s-local-setup.sh

# Build and deploy
./scripts/k8s-deploy.sh --build

# Connect
kubectl port-forward -n pdb svc/pdb-postgres 5432:5432
psql -h localhost -U pdb_admin -d pdb
```

## Core Functions

| Function                                     | Description                         |
| -------------------------------------------- | ----------------------------------- |
| `create_bm25_index(table, column)`           | Create index with auto-sync trigger |
| `drop_bm25_index(table, column)`             | Remove index and trigger            |
| `bm25_search(table, column, query)`          | Search, returns `(pk, score)`       |
| `bm25_search_limit(table, column, query, n)` | Search with result limit            |
| `bm25_index_info(table, column)`             | Index stats (doc count, size)       |

## `@@@` Operator

The `@@@` operator enables natural SQL search syntax in WHERE clauses:

```sql
-- Step 1: Initialize search context
SELECT pdb_search_init('articles', 'body', 'rust programming');

-- Step 2: Use @@@ in WHERE clause with scoring
SELECT id, title, pdb_operator_score(id) as score
FROM articles
WHERE body @@@ 'rust'
  AND pdb_operator_score(id) > 0
ORDER BY score DESC;
```

**How it works:**

1. `pdb_search_init()` executes the search and caches matching PKs
2. `@@@` filters rows with non-null content
3. `pdb_operator_score(pk)` returns the BM25 score (0 = no match)

**Operator Functions:**

| Function                                | Description                      |
| --------------------------------------- | -------------------------------- |
| `pdb_search_init(table, column, query)` | Initialize search context        |
| `pdb_operator_score(pk)`                | Get score for row (0 = no match) |
| `body @@@ 'query'`                      | Filter in WHERE clause           |

## Query Syntax

Standard Tantivy query syntax:

```sql
SELECT pdb_search_init('articles', 'body', 'rust AND fast');     -- Boolean
SELECT pdb_search_init('articles', 'body', '"rust programming"'); -- Phrase
SELECT pdb_search_init('articles', 'body', 'rust -python');      -- Exclusion
SELECT pdb_search_init('articles', 'body', 'rust^2 python');     -- Boost
```

## Alternative: Function-Based Search

If you prefer JOINs over the operator pattern:

```sql
SELECT a.id, a.title, s.score
FROM articles a
JOIN bm25_search('articles', 'body', 'rust') s ON a.id = s.pk
ORDER BY s.score DESC;
```

## Inline Search (pdb_match)

For simpler inline filtering without the operator:

```sql
SELECT id, title, pdb_score('articles', 'body', 'rust', id) as score
FROM articles
WHERE pdb_match('articles', 'body', 'rust', id)
  AND author = 'John'
ORDER BY score DESC;
```

## How It Works

**Architecture:**

```
┌─────────────┐       ┌─────────────┐       ┌─────────────┐
│  PostgreSQL │◄─────►│    pgrx     │◄─────►│   Tantivy   │
│    Table    │       │  Extension  │       │   Indexes   │
└─────────────┘       └─────────────┘       └─────────────┘
       │                     │                     │
       │                     │                     │
       ▼                     ▼                     ▼
   CRUD ops             Triggers              BM25 Search
                       (buffered)             (on commit)
```

**Key Implementation Details:**

1. **Index storage**: `$PGDATA/pdb_indexes/<table>_<column>/`
2. **Schema**: Tantivy indexes store `pk` (i64) and `content` (text)
3. **Transaction safety**: Operations buffered in `writer.rs`, flushed on PostgreSQL COMMIT via `register_xact_callback`
4. **Trigger**: `pdb_sync_trigger()` fires on INSERT/UPDATE/DELETE, extracts PK from tuple

## Requirements

- PostgreSQL 13-17
- Table must have a **single-column integer primary key** (int, bigint, serial)
- Text column must be `TEXT` type

## Project Structure

```
pdb/
├── src/
│   ├── index.rs      # create/drop/refresh indexes
│   ├── search.rs     # bm25_search functions
│   ├── trigger.rs    # auto-sync trigger
│   ├── writer.rs     # transaction-safe buffered writes
│   ├── operator.rs   # inline search (pdb_match, pdb_score)
│   └── utils.rs      # index info
├── k8s/              # Kubernetes manifests (Kustomize)
│   ├── base/         # Common resources
│   └── overlays/     # dev, aws configs
├── terraform/        # AWS EC2 + k3s deployment
├── tests/            # SQL test scripts
├── Dockerfile        # Multi-stage build
└── docker-compose.yml
```

## Infrastructure

**Kubernetes (k8s/):**

- StatefulSet with persistent volumes for data and indexes
- Configurable via Kustomize overlays
- Includes health checks, PodDisruptionBudget

**Terraform (terraform/):**

- Deploys t3.micro EC2 with k3s
- ECR for container registry
- VPC, security groups configured

## Tests

```bash
# Run E2E tests in psql
\i tests/test_trigger_e2e.sql     # Transaction safety
\i tests/test_inline_search.sql   # pdb_match/pdb_score
\i tests/test_operator.sql        # @@@ operator
\i tests/test_complete_flow.sql   # Full workflow
```

## License

MIT
