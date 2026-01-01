#!/bin/bash
# =============================================================================
# k8s-deploy.sh - Deploy pdb to Kubernetes
# =============================================================================
# Deploys the pdb PostgreSQL extension to a Kubernetes cluster.
# Works with kind, k3s, or any K8s cluster.
#
# Usage:
#   ./scripts/k8s-deploy.sh              # Deploy only (image must exist)
#   ./scripts/k8s-deploy.sh --build      # Build image, load, and deploy
#   ./scripts/k8s-deploy.sh --delete     # Delete all pdb resources
#
# Environment variables:
#   CLUSTER_NAME  - kind cluster name (default: pdb-cluster)
#   IMAGE_NAME    - Docker image name (default: pdb-postgres)
#   IMAGE_TAG     - Docker image tag (default: local)
# =============================================================================

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CLUSTER_NAME="${CLUSTER_NAME:-pdb-cluster}"
IMAGE_NAME="${IMAGE_NAME:-pdb-postgres}"
IMAGE_TAG="${IMAGE_TAG:-local}"
NAMESPACE="pdb"
OVERLAY="${OVERLAY:-dev}"  # Use dev overlay by default

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}ℹ️  $1${NC}"; }
log_success() { echo -e "${GREEN}✅ $1${NC}"; }
log_warning() { echo -e "${YELLOW}⚠️  $1${NC}"; }
log_error() { echo -e "${RED}❌ $1${NC}"; }

# Parse arguments
BUILD=false
DELETE=false
for arg in "$@"; do
    case $arg in
        --build) BUILD=true ;;
        --delete) DELETE=true ;;
        --help|-h)
            echo "Usage: $0 [--build] [--delete]"
            echo ""
            echo "Options:"
            echo "  --build   Build Docker image before deploying"
            echo "  --delete  Delete all pdb resources from cluster"
            echo "  --help    Show this help message"
            exit 0
            ;;
    esac
done

echo ""
echo "======================================"
echo "PDB Kubernetes Deployment"
echo "======================================"
echo ""

# Handle delete
if $DELETE; then
    log_warning "Deleting all pdb resources..."
    kubectl delete namespace "$NAMESPACE" --ignore-not-found=true
    log_success "Resources deleted"
    exit 0
fi

# Check prerequisites
if ! command -v kubectl &> /dev/null; then
    log_error "kubectl is not installed. Run: ./scripts/k8s-local-setup.sh"
    exit 1
fi

# Check if we're using kind
USING_KIND=false
if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
    USING_KIND=true
    # Set kubectl context to kind cluster
    kubectl config use-context "kind-${CLUSTER_NAME}" &>/dev/null || true
fi

# Verify cluster connection
if ! kubectl cluster-info &>/dev/null; then
    log_error "Cannot connect to Kubernetes cluster."
    echo "   Run: ./scripts/k8s-local-setup.sh"
    exit 1
fi

log_success "Connected to cluster"

# Build image if requested
if $BUILD; then
    echo ""
    log_info "Building Docker image..."
    cd "$PROJECT_DIR"
    docker build -t "${IMAGE_NAME}:${IMAGE_TAG}" .
    log_success "Image built: ${IMAGE_NAME}:${IMAGE_TAG}"
    
    # Load into kind if using kind
    if $USING_KIND; then
        echo ""
        log_info "Loading image into kind cluster..."
        kind load docker-image "${IMAGE_NAME}:${IMAGE_TAG}" --name "$CLUSTER_NAME"
        log_success "Image loaded into kind"
    fi
fi

# Deploy manifests
echo ""
log_info "Deploying K8s manifests (overlay: $OVERLAY)..."
cd "$PROJECT_DIR"
kubectl apply -k "k8s/overlays/${OVERLAY}"
log_success "Manifests applied"

# Wait for pod to be ready
echo ""
log_info "Waiting for pod to be ready (this may take a few minutes on first run)..."
kubectl wait --for=condition=Ready pod/pdb-postgres-0 -n "$NAMESPACE" --timeout=300s || {
    log_error "Pod failed to become ready. Checking events..."
    echo ""
    kubectl describe pod pdb-postgres-0 -n "$NAMESPACE" | tail -30
    exit 1
}
log_success "Pod is ready!"

# Create extension
echo ""
log_info "Creating pdb extension..."
kubectl exec -n "$NAMESPACE" pdb-postgres-0 -- \
    psql -U pdb_admin -d pdb -c "CREATE EXTENSION IF NOT EXISTS pdb;" 2>/dev/null || {
    log_warning "Extension may already exist or failed to create. Check logs."
}
log_success "Extension created"

# Print connection info
echo ""
echo "======================================"
echo -e "${GREEN}Deployment Complete!${NC}"
echo "======================================"
echo ""
echo "📊 Resources:"
kubectl get all -n "$NAMESPACE" | sed 's/^/   /'
echo ""
echo "📦 PVCs:"
kubectl get pvc -n "$NAMESPACE" | sed 's/^/   /'
echo ""
echo "======================================"
echo "🔗 Connect to PostgreSQL:"
echo "======================================"
echo ""
echo "Option 1: Port forward (recommended)"
echo "   kubectl port-forward -n $NAMESPACE svc/pdb-postgres 5432:5432"
echo "   psql -h localhost -U pdb_admin -d pdb"
echo ""
echo "Option 2: Exec into pod"
echo "   kubectl exec -it -n $NAMESPACE pdb-postgres-0 -- psql -U pdb_admin -d pdb"
echo ""
echo "📋 Useful commands:"
echo "   kubectl logs -n $NAMESPACE pdb-postgres-0 -f    # View logs"
echo "   kubectl describe pod -n $NAMESPACE pdb-postgres-0  # Pod details"
echo "   ./scripts/k8s-deploy.sh --delete                # Clean up"
echo "======================================"
