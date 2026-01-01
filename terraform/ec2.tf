# ============================================
# EC2 Instance with k3s
# ============================================

# Debian 12 (Bookworm) AMI - matches Dockerfile base image
data "aws_ami" "debian" {
  most_recent = true
  owners      = ["136693071363"]  # Debian official

  filter {
    name   = "name"
    values = ["debian-12-amd64-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "architecture"
    values = ["x86_64"]
  }
}

# IAM Role for EC2 (ECR access + SSM)
resource "aws_iam_role" "ec2" {
  name = "pdb-ec2-role-${var.environment}"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ecr_readonly" {
  role       = aws_iam_role.ec2.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly"
}

resource "aws_iam_role_policy_attachment" "ssm" {
  role       = aws_iam_role.ec2.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"
}

resource "aws_iam_instance_profile" "ec2" {
  name = "pdb-ec2-profile-${var.environment}"
  role = aws_iam_role.ec2.name
}

# EC2 Instance
resource "aws_instance" "k3s" {
  ami                    = data.aws_ami.debian.id
  instance_type          = var.instance_type
  subnet_id              = aws_subnet.public.id
  vpc_security_group_ids = [aws_security_group.k3s.id]
  iam_instance_profile   = aws_iam_instance_profile.ec2.name
  key_name               = var.ssh_key_name

  # Root volume (OS + k3s + container images)
  root_block_device {
    volume_size           = 20  # Within 30GB free tier
    volume_type           = "gp3"
    encrypted             = true
    delete_on_termination = true
  }

  user_data = base64encode(templatefile("${path.module}/scripts/user-data.sh", {
    aws_region        = var.aws_region
    ecr_repo_url      = aws_ecr_repository.pdb.repository_url
    postgres_password = var.postgres_password
  }))

  tags = {
    Name = "pdb-k3s-${var.environment}"
  }

  # Allow user-data to complete before marking as created
  lifecycle {
    create_before_destroy = true
  }
}

# Elastic IP (free while attached to running instance)
resource "aws_eip" "k3s" {
  instance = aws_instance.k3s.id
  domain   = "vpc"

  tags = {
    Name = "pdb-eip-${var.environment}"
  }
}

# EBS Volume for PostgreSQL data (survives instance termination)
resource "aws_ebs_volume" "pgdata" {
  availability_zone = aws_instance.k3s.availability_zone
  size              = 8  # Within 30GB free tier (20 root + 8 data = 28GB)
  type              = "gp3"
  encrypted         = true

  tags = {
    Name = "pdb-pgdata-${var.environment}"
  }
}

resource "aws_volume_attachment" "pgdata" {
  device_name = "/dev/xvdf"
  volume_id   = aws_ebs_volume.pgdata.id
  instance_id = aws_instance.k3s.id
}
