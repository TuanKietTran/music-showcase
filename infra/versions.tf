terraform {
  required_version = ">= 1.6"
  required_providers {
    aws    = { source = "hashicorp/aws", version = "~> 6.0" }
    random = { source = "hashicorp/random", version = "~> 3.6" }
  }
  # Local state by default. For anything shared, see the "Remote state" section of README.md
  # (an S3 backend is NOT created by this configuration).
}

# Guard rail: refuse to run against any account other than the one you confirmed.
provider "aws" {
  region              = var.region
  allowed_account_ids = [var.expected_account_id]
  default_tags {
    tags = { app = "music-showcase", env = "dev" }
  }
}

# CloudFront certificates and the (optional) CloudFront WAF must live in us-east-1.
provider "aws" {
  alias               = "use1"
  region              = "us-east-1"
  allowed_account_ids = [var.expected_account_id]
  default_tags {
    tags = { app = "music-showcase", env = "dev" }
  }
}
