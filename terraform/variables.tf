# ============================================
# Input Variables
# ============================================

variable "aws_region" {
  description = "AWS region to deploy resources"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string
  default     = "dev"

  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}

variable "instance_type" {
  description = "EC2 instance type (t3.micro for free tier)"
  type        = string
  default     = "t3.micro"
}

variable "ssh_key_name" {
  description = "Name of existing SSH key pair in AWS"
  type        = string
  default     = "kv's macbook"
}

variable "postgres_password" {
  description = "PostgreSQL admin password (generate with: openssl rand -base64 24)"
  type        = string
  sensitive   = true
}

variable "allowed_cidrs" {
  description = "CIDR blocks allowed SSH/K8s/Postgres access (e.g., your IP: [\"1.2.3.4/32\"])"
  type        = list(string)

  validation {
    condition     = length(var.allowed_cidrs) > 0
    error_message = "You must specify at least one CIDR for security. Use your IP/32."
  }
}
