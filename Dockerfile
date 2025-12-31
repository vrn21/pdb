# ============================================
# Stage 1: Chef Base
# ============================================
FROM postgres:17-bookworm AS chef

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    clang \
    libssl-dev \
    curl \
    ca-certificates \
    postgresql-server-dev-17 \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- --default-toolchain stable -y
ENV PATH="/root/.cargo/bin:$PATH"

# Install cargo-chef and pgrx
ARG PGRX_VERSION=0.16.1
RUN cargo install cargo-chef --locked \
    && cargo install cargo-pgrx --version ${PGRX_VERSION} --locked

# Initialize pgrx with system PostgreSQL
RUN cargo pgrx init --pg17=/usr/lib/postgresql/17/bin/pg_config

WORKDIR /build

# ============================================
# Stage 2: Planner (generate recipe.json)
# ============================================
FROM chef AS planner

COPY Cargo.toml pdb.control ./
COPY src ./src

# Generate dependency recipe
RUN cargo chef prepare --recipe-path recipe.json

# ============================================
# Stage 3: Builder (cook dependencies + build)
# ============================================
FROM chef AS builder

# Copy and cook dependencies (this layer is cached!)
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --features pg17 --recipe-path recipe.json

# Copy source and build extension
COPY Cargo.toml pdb.control ./
COPY src ./src

RUN cargo pgrx package --pg-config /usr/lib/postgresql/17/bin/pg_config

# ============================================
# Stage 4: Runtime
# ============================================
FROM postgres:17-bookworm

# Copy extension artifacts
COPY --from=builder \
    /build/target/release/pdb-pg17/usr/share/postgresql/17/extension/pdb* \
    /usr/share/postgresql/17/extension/
COPY --from=builder \
    /build/target/release/pdb-pg17/usr/lib/postgresql/17/lib/pdb.so \
    /usr/lib/postgresql/17/lib/

# Create Tantivy index directory
RUN mkdir -p /var/lib/pdb_indexes \
    && chown postgres:postgres /var/lib/pdb_indexes \
    && chmod 700 /var/lib/pdb_indexes

ENV PDB_INDEX_PATH=/var/lib/pdb_indexes

HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=3 \
    CMD pg_isready -U "${POSTGRES_USER:-postgres}" || exit 1

EXPOSE 5432
