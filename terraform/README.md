# PDB Terraform Deployment

This directory contains Terraform configuration to deploy the pdb PostgreSQL extension on AWS using a **free-tier** EC2 instance running k3s (lightweight Kubernetes).

## Cost: $0/month (Free Tier)

| Resource     | Free Tier Limit | Usage      |
| ------------ | --------------- | ---------- |
| EC2 t3.micro | 750 hrs/month   | 1 instance |
| EBS gp3      | 30 GB           | 28 GB      |
| ECR          | 500 MB          | ~100 MB    |

## Prerequisites

1. AWS CLI configured: `aws configure`
2. Terraform installed: `brew install terraform`
3. Your public IP for access: `curl -s ifconfig.me`

## Quick Start

```bash
# 1. Create terraform.tfvars
cp terraform.tfvars.example terraform.tfvars

# 2. Edit terraform.tfvars
#    - Set postgres_password (generate: openssl rand -base64 24)
#    - Set allowed_cidrs to your IP (e.g., ["1.2.3.4/32"])

# 3. Initialize and deploy
terraform init
terraform apply

# 4. Wait 3-5 minutes for k3s to initialize

# 5. Follow the workflow printed in outputs
terraform output full_deployment_workflow
```

## Files

| File                   | Purpose                       |
| ---------------------- | ----------------------------- |
| `main.tf`              | Provider configuration        |
| `variables.tf`         | Input variables               |
| `vpc.tf`               | VPC, subnet, internet gateway |
| `security.tf`          | Security group rules          |
| `ec2.tf`               | EC2 instance, EBS volumes     |
| `ecr.tf`               | Container registry            |
| `outputs.tf`           | Connection commands           |
| `scripts/user-data.sh` | k3s bootstrap script          |

## Connecting

```bash
# SSH into instance
ssh -i ~/.ssh/kv_macbook.pem admin@$(terraform output -raw public_ip)

# Get kubeconfig
scp -i ~/.ssh/kv_macbook.pem admin@$(terraform output -raw public_ip):/etc/rancher/k3s/k3s.yaml ./kubeconfig-aws
sed -i '' "s/127.0.0.1/$(terraform output -raw public_ip)/g" ./kubeconfig-aws
export KUBECONFIG=./kubeconfig-aws

# Deploy pdb
kubectl apply -k ../k8s/overlays/aws
```

## Cleanup

```bash
terraform destroy
```
