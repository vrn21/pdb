#!/bin/bash
# =============================================================================
# k8s-local-setup.sh - Setup local Kubernetes environment
# =============================================================================
# This script installs kind (Kubernetes IN Docker) for local development.
# kind is recommended for Mac users as it works well with Docker Desktop.
#
# Usage:
#   ./scripts/k8s-local-setup.sh
# =============================================================================

set -euo pipefail

CLUSTER_NAME="${CLUSTER_NAME:-pdb-cluster}"

echo "======================================"
echo "PDB Local Kubernetes Setup"
echo "======================================"
echo ""

# Check for Docker
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed."
    echo "   Please install Docker Desktop from: https://www.docker.com/products/docker-desktop"
    exit 1
fi

echo "✅ Docker is installed"

# Check if Docker is running
if ! docker info &> /dev/null; then
    echo "❌ Docker is not running. Please start Docker Desktop."
    exit 1
fi

echo "✅ Docker is running"

# Check for kubectl
if ! command -v kubectl &> /dev/null; then
    echo ""
    echo "📦 Installing kubectl..."
    if [[ "$OSTYPE" == "darwin"* ]]; then
        brew install kubectl
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
        chmod +x kubectl
        sudo mv kubectl /usr/local/bin/
    fi
fi

echo "✅ kubectl is installed: $(kubectl version --client --short 2>/dev/null || kubectl version --client)"

# Check for kind
if ! command -v kind &> /dev/null; then
    echo ""
    echo "📦 Installing kind..."
    if [[ "$OSTYPE" == "darwin"* ]]; then
        brew install kind
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
        chmod +x ./kind
        sudo mv ./kind /usr/local/bin/kind
    fi
fi

echo "✅ kind is installed: $(kind version)"

# Check if cluster already exists
if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
    echo ""
    echo "ℹ️  Cluster '${CLUSTER_NAME}' already exists."
    read -p "   Delete and recreate? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "🗑️  Deleting existing cluster..."
        kind delete cluster --name "$CLUSTER_NAME"
    else
        echo "   Using existing cluster."
        kubectl cluster-info --context "kind-${CLUSTER_NAME}"
        exit 0
    fi
fi

# Create kind cluster
echo ""
echo "🚀 Creating kind cluster '${CLUSTER_NAME}'..."
kind create cluster --name "$CLUSTER_NAME" --wait 60s

# Verify cluster
echo ""
echo "✅ Cluster created successfully!"
echo ""
kubectl cluster-info --context "kind-${CLUSTER_NAME}"

echo ""
echo "======================================"
echo "Next steps:"
echo "======================================"
echo "1. Build the Docker image:"
echo "   docker build -t pdb-postgres:local ."
echo ""
echo "2. Load image into kind:"
echo "   kind load docker-image pdb-postgres:local --name ${CLUSTER_NAME}"
echo ""
echo "3. Deploy to cluster:"
echo "   ./scripts/k8s-deploy.sh"
echo ""
echo "Or run all at once:"
echo "   ./scripts/k8s-deploy.sh --build"
echo "======================================"
