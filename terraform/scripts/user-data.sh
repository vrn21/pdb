#!/bin/bash
# ============================================
# User Data Script - Bootstrap k3s on Debian 12
# ============================================
# This script runs on first boot of the EC2 instance.
# It installs k3s, mounts EBS volumes, and prepares
# Kubernetes resources for the pdb deployment.
# ============================================

set -euxo pipefail

# Redirect all output to log file
exec > >(tee /var/log/user-data.log) 2>&1

echo "=========================================="
echo "PDB k3s Bootstrap Script"
echo "Started at: $(date)"
echo "=========================================="

# Variables injected by Terraform
AWS_REGION="${aws_region}"
ECR_REPO_URL="${ecr_repo_url}"
POSTGRES_PASSWORD="${postgres_password}"

# ============================================
# Step 1: Install dependencies
# ============================================
echo ">>> Installing dependencies..."
apt-get update
apt-get install -y curl unzip jq

# Install AWS CLI v2
echo ">>> Installing AWS CLI..."
curl -fsSL "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"
unzip -q awscliv2.zip
./aws/install
rm -rf aws awscliv2.zip

# ============================================
# Step 2: Install k3s
# ============================================
echo ">>> Installing k3s..."
curl -sfL https://get.k3s.io | sh -s - \
    --write-kubeconfig-mode 644 \
    --disable traefik \
    --disable servicelb \
    --kubelet-arg="eviction-hard=memory.available<100Mi,nodefs.available<1Gi"

# Wait for k3s to be ready
echo ">>> Waiting for k3s to be ready..."
sleep 30
until kubectl get nodes 2>/dev/null; do
    echo "Waiting for k3s..."
    sleep 10
done

echo ">>> k3s is ready!"
kubectl get nodes

# ============================================
# Step 3: Mount EBS volume for PostgreSQL data
# ============================================
echo ">>> Setting up EBS volume..."

# Wait for EBS volume to attach
echo "Waiting for EBS volume /dev/xvdf..."
while [ ! -e /dev/xvdf ] && [ ! -e /dev/nvme1n1 ]; do
    sleep 5
done

# Determine the actual device name (can vary by instance type)
if [ -e /dev/nvme1n1 ]; then
    EBS_DEVICE="/dev/nvme1n1"
else
    EBS_DEVICE="/dev/xvdf"
fi

echo "EBS device found: $EBS_DEVICE"

# Format if not already formatted
if ! blkid "$EBS_DEVICE"; then
    echo "Formatting $EBS_DEVICE as ext4..."
    mkfs.ext4 -L pgdata "$EBS_DEVICE"
fi

# Create mount points
mkdir -p /data/pgdata
mkdir -p /data/pdb-indexes

# Mount EBS volume
mount "$EBS_DEVICE" /data/pgdata
echo "LABEL=pgdata /data/pgdata ext4 defaults,nofail 0 2" >> /etc/fstab

# Set ownership for postgres user (UID 999 in official postgres image)
chown -R 999:999 /data/pgdata /data/pdb-indexes
chmod 700 /data/pgdata /data/pdb-indexes

echo ">>> EBS volume mounted at /data/pgdata"

# ============================================
# Step 4: Create Kubernetes namespace and secret
# ============================================
echo ">>> Creating Kubernetes resources..."

# Create namespace
kubectl create namespace pdb || true

# Create secret for postgres credentials
kubectl create secret generic pdb-secrets \
    --namespace=pdb \
    --from-literal=POSTGRES_USER=pdb_admin \
    --from-literal=POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    --dry-run=client -o yaml | kubectl apply -f -

# ============================================
# Step 5: Create local PersistentVolumes
# ============================================
echo ">>> Creating PersistentVolumes..."

cat <<EOF | kubectl apply -f -
---
apiVersion: v1
kind: PersistentVolume
metadata:
  name: pdb-pgdata-pv
  labels:
    type: local
    app: pdb
spec:
  capacity:
    storage: 8Gi
  accessModes:
    - ReadWriteOnce
  persistentVolumeReclaimPolicy: Retain
  storageClassName: local-storage
  local:
    path: /data/pgdata
  nodeAffinity:
    required:
      nodeSelectorTerms:
        - matchExpressions:
            - key: kubernetes.io/hostname
              operator: Exists
---
apiVersion: v1
kind: PersistentVolume
metadata:
  name: pdb-indexes-pv
  labels:
    type: local
    app: pdb
spec:
  capacity:
    storage: 5Gi
  accessModes:
    - ReadWriteOnce
  persistentVolumeReclaimPolicy: Retain
  storageClassName: local-storage
  local:
    path: /data/pdb-indexes
  nodeAffinity:
    required:
      nodeSelectorTerms:
        - matchExpressions:
            - key: kubernetes.io/hostname
              operator: Exists
---
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: local-storage
provisioner: kubernetes.io/no-provisioner
volumeBindingMode: WaitForFirstConsumer
EOF

echo ">>> PersistentVolumes created"

# ============================================
# Step 6: Verify setup
# ============================================
echo ">>> Verifying setup..."
kubectl get nodes
kubectl get pv
kubectl get storageclass
kubectl get secrets -n pdb

echo "=========================================="
echo "PDB k3s Bootstrap Complete!"
echo "Finished at: $(date)"
echo "=========================================="
echo ""
echo "Next steps:"
echo "1. Copy kubeconfig: scp admin@<IP>:/etc/rancher/k3s/k3s.yaml ./kubeconfig"
echo "2. Update server address in kubeconfig"
echo "3. Deploy with: kubectl apply -k k8s/overlays/aws"
