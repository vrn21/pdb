# ============================================
# Security Group
# ============================================

resource "aws_security_group" "k3s" {
  name        = "pdb-k3s-${var.environment}"
  description = "Security group for pdb k3s node"
  vpc_id      = aws_vpc.main.id

  # SSH access
  ingress {
    description = "SSH"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidrs
  }

  # Kubernetes API (k3s)
  ingress {
    description = "K8s API"
    from_port   = 6443
    to_port     = 6443
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidrs
  }

  # PostgreSQL (NodePort range includes 30000-32767, but also direct 5432)
  ingress {
    description = "PostgreSQL direct"
    from_port   = 5432
    to_port     = 5432
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidrs
  }

  # NodePort range for K8s services
  ingress {
    description = "K8s NodePort range"
    from_port   = 30000
    to_port     = 32767
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidrs
  }

  # All outbound traffic
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "pdb-k3s-sg-${var.environment}"
  }
}
