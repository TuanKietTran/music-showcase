data "aws_caller_identity" "current" {}
data "aws_vpc" "default" { default = true }
data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }
  filter {
    name   = "default-for-az"
    values = ["true"]
  }
}

locals {
  name        = "music-showcase"
  account_id  = data.aws_caller_identity.current.account_id
  use_domain  = var.domain_zone != ""
  host        = local.use_domain ? "${var.subdomain}.${var.domain_zone}" : ""
  origin_host = local.use_domain ? "origin-${var.subdomain}.${var.domain_zone}" : ""
  app_origin  = local.use_domain ? "https://${local.host}" : "https://${aws_cloudfront_distribution.main.domain_name}"
}

resource "random_password" "admin_token" {
  length  = 40
  special = false
}

resource "random_password" "origin_verify" {
  length  = 40
  special = false
}

resource "random_string" "pool_domain" {
  length  = 8
  special = false
  upper   = false
}

# Break-glass bootstrap admin token (MUSIC_TOKEN). Stored as SecureString and injected into the task by ECS;
# it is never in the image, the task definition's plain environment, or the repo. (Terraform state holds it: keep state private.)
resource "aws_ssm_parameter" "admin_token" {
  name  = "/${local.name}/dev/token"
  type  = "SecureString"
  value = random_password.admin_token.result
}

resource "aws_dynamodb_table" "uploads" {
  name         = "music-uploads"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "ownerId"
  range_key    = "id"
  attribute {
    name = "ownerId"
    type = "S"
  }
  attribute {
    name = "id"
    type = "S"
  }
  point_in_time_recovery { enabled = true }
  server_side_encryption { enabled = true }
}
