# ============================================
# ECR Repository (500MB free/month)
# ============================================

resource "aws_ecr_repository" "pdb" {
  name                 = "pdb-postgres"
  image_tag_mutability = "MUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }

  encryption_configuration {
    encryption_type = "AES256"
  }

  tags = {
    Name = "pdb-postgres-${var.environment}"
  }
}

# Keep only last 3 images to minimize storage
resource "aws_ecr_lifecycle_policy" "pdb" {
  repository = aws_ecr_repository.pdb.name

  policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Keep last 3 images"
      selection = {
        tagStatus   = "any"
        countType   = "imageCountMoreThan"
        countNumber = 3
      }
      action = {
        type = "expire"
      }
    }]
  })
}
