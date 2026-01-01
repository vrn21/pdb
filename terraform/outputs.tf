# ============================================
# Outputs
# ============================================

output "public_ip" {
  description = "Public IP address of the k3s node"
  value       = aws_eip.k3s.public_ip
}

output "ssh_command" {
  description = "SSH command to connect to the instance"
  value       = "ssh -i ~/.ssh/kv_macbook.pem admin@${aws_eip.k3s.public_ip}"
}

output "ecr_repository_url" {
  description = "ECR repository URL for pushing images"
  value       = aws_ecr_repository.pdb.repository_url
}

output "ecr_login_command" {
  description = "Command to login to ECR"
  value       = "aws ecr get-login-password --region ${var.aws_region} | docker login --username AWS --password-stdin ${aws_ecr_repository.pdb.repository_url}"
}

output "kubeconfig_commands" {
  description = "Commands to get kubeconfig from k3s"
  value       = <<-EOT
    # 1. Copy kubeconfig from remote server
    scp -i ~/.ssh/kv_macbook.pem admin@${aws_eip.k3s.public_ip}:/etc/rancher/k3s/k3s.yaml ./kubeconfig-aws
    
    # 2. Update the server address
    sed -i '' 's/127.0.0.1/${aws_eip.k3s.public_ip}/g' ./kubeconfig-aws
    
    # 3. Use it
    export KUBECONFIG=./kubeconfig-aws
    kubectl get nodes
  EOT
}

output "full_deployment_workflow" {
  description = "Complete workflow to deploy pdb"
  value       = <<-EOT
    ╔════════════════════════════════════════════════════════════════╗
    ║                    PDB Deployment Workflow                      ║
    ╚════════════════════════════════════════════════════════════════╝
    
    Step 1: Wait for k3s to initialize (~3-5 minutes after terraform apply)
    ────────────────────────────────────────────────────────────────────
    ssh -i ~/.ssh/kv_macbook.pem admin@${aws_eip.k3s.public_ip}
    # Then run: sudo kubectl get nodes
    # Wait until you see the node in "Ready" state
    
    Step 2: Get kubeconfig
    ────────────────────────────────────────────────────────────────────
    scp -i ~/.ssh/kv_macbook.pem admin@${aws_eip.k3s.public_ip}:/etc/rancher/k3s/k3s.yaml ./kubeconfig-aws
    sed -i '' 's/127.0.0.1/${aws_eip.k3s.public_ip}/g' ./kubeconfig-aws
    export KUBECONFIG=./kubeconfig-aws
    
    Step 3: Build and push Docker image
    ────────────────────────────────────────────────────────────────────
    docker build -t pdb-postgres:17 .
    aws ecr get-login-password --region ${var.aws_region} | docker login --username AWS --password-stdin ${aws_ecr_repository.pdb.repository_url}
    docker tag pdb-postgres:17 ${aws_ecr_repository.pdb.repository_url}:17
    docker push ${aws_ecr_repository.pdb.repository_url}:17
    
    Step 4: Deploy using Kustomize
    ────────────────────────────────────────────────────────────────────
    kubectl apply -k k8s/overlays/aws
    
    Step 5: Wait and connect
    ────────────────────────────────────────────────────────────────────
    kubectl wait --for=condition=Ready pod/pdb-postgres-0 -n pdb --timeout=300s
    kubectl port-forward -n pdb svc/pdb-postgres 5432:5432
    # In another terminal:
    psql -h localhost -U pdb_admin -d pdb
  EOT
}
