locals {
  media_bucket = "${local.name}-media-${local.account_id}-${var.region}"
  logs_bucket  = "${local.name}-logs-${local.account_id}-${var.region}"
  web_bucket   = "${local.name}-web-${local.account_id}-${var.region}"
}

# ---------------------------------------------------------------- access logs + CloudTrail destination
resource "aws_s3_bucket" "logs" { bucket = local.logs_bucket }

resource "aws_s3_bucket_public_access_block" "logs" {
  bucket                  = aws_s3_bucket.logs.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}
resource "aws_s3_bucket_ownership_controls" "logs" {
  bucket = aws_s3_bucket.logs.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}
resource "aws_s3_bucket_server_side_encryption_configuration" "logs" {
  bucket = aws_s3_bucket.logs.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  } # S3 server access logging cannot write to KMS-encrypted buckets
}
resource "aws_s3_bucket_lifecycle_configuration" "logs" {
  bucket = aws_s3_bucket.logs.id
  rule {
    id     = "expire-logs"
    status = "Enabled"
    filter {}
    expiration { days = 90 }
    abort_incomplete_multipart_upload { days_after_initiation = 1 }
  }
}
resource "aws_s3_bucket_policy" "logs" {
  bucket = aws_s3_bucket.logs.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Sid = "DenyInsecureTransport", Effect = "Deny", Principal = "*", Action = "s3:*", Resource = [aws_s3_bucket.logs.arn, "${aws_s3_bucket.logs.arn}/*"], Condition = { Bool = { "aws:SecureTransport" = "false" } } },
      { Sid = "S3ServerAccessLogs", Effect = "Allow", Principal = { Service = "logging.s3.amazonaws.com" }, Action = "s3:PutObject", Resource = "${aws_s3_bucket.logs.arn}/s3-access/*",
      Condition = { StringEquals = { "aws:SourceAccount" = local.account_id }, ArnLike = { "aws:SourceArn" = aws_s3_bucket.media.arn } } },
      { Sid = "CloudTrailAclCheck", Effect = "Allow", Principal = { Service = "cloudtrail.amazonaws.com" }, Action = "s3:GetBucketAcl", Resource = aws_s3_bucket.logs.arn,
      Condition = { StringEquals = { "aws:SourceArn" = local.trail_arn } } },
      { Sid = "CloudTrailWrite", Effect = "Allow", Principal = { Service = "cloudtrail.amazonaws.com" }, Action = "s3:PutObject", Resource = "${aws_s3_bucket.logs.arn}/cloudtrail/AWSLogs/${local.account_id}/*",
      Condition = { StringEquals = { "s3:x-amz-acl" = "bucket-owner-full-control", "aws:SourceArn" = local.trail_arn } } },
    ]
  })
  depends_on = [aws_s3_bucket_public_access_block.logs]
}

# ---------------------------------------------------------------- media bucket (private, versioned, SSE-KMS CMK)
resource "aws_s3_bucket" "media" { bucket = local.media_bucket } # force_destroy stays false: scripts/aws-destroy.sh empties versions explicitly

resource "aws_s3_bucket_public_access_block" "media" {
  bucket                  = aws_s3_bucket.media.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}
resource "aws_s3_bucket_ownership_controls" "media" {
  bucket = aws_s3_bucket.media.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}
resource "aws_s3_bucket_versioning" "media" {
  bucket = aws_s3_bucket.media.id
  versioning_configuration {
    status = "Enabled"
  }
}
resource "aws_s3_bucket_server_side_encryption_configuration" "media" {
  bucket = aws_s3_bucket.media.id
  rule {
    bucket_key_enabled = true
    apply_server_side_encryption_by_default {
      sse_algorithm     = "aws:kms"
      kms_master_key_id = aws_kms_key.media.arn
    }
  }
}
resource "aws_s3_bucket_logging" "media" {
  bucket        = aws_s3_bucket.media.id
  target_bucket = aws_s3_bucket.logs.id
  target_prefix = "s3-access/media/"
  depends_on    = [aws_s3_bucket_policy.logs]
}
resource "aws_s3_bucket_lifecycle_configuration" "media" {
  bucket = aws_s3_bucket.media.id
  rule {
    id     = "noncurrent-30d-and-abort-multipart"
    status = "Enabled"
    filter {}
    noncurrent_version_expiration { noncurrent_days = 30 }
    abort_incomplete_multipart_upload { days_after_initiation = 1 }
  }
  rule {
    id     = "clean-delete-markers"
    status = "Enabled"
    filter {}
    expiration { expired_object_delete_marker = true }
  }
  depends_on = [aws_s3_bucket_versioning.media]
}
resource "aws_s3_bucket_cors_configuration" "media" {
  bucket = aws_s3_bucket.media.id
  cors_rule {
    allowed_origins = [local.app_origin]
    allowed_methods = ["GET", "HEAD"]
    allowed_headers = ["Range"]
    expose_headers  = ["Content-Range", "Accept-Ranges"]
    max_age_seconds = 3600
  }
}
# Request metrics so the S3 4xx alarm has data.
resource "aws_s3_bucket_metric" "media" {
  bucket = aws_s3_bucket.media.id
  name   = "EntireBucket"
}

resource "aws_s3_bucket_policy" "media" {
  bucket = aws_s3_bucket.media.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Sid = "DenyInsecureTransport", Effect = "Deny", Principal = "*", Action = "s3:*", Resource = [aws_s3_bucket.media.arn, "${aws_s3_bucket.media.arn}/*"], Condition = { Bool = { "aws:SecureTransport" = "false" } } },
      # Every object write must request SSE-KMS explicitly (a header-less put is refused rather than silently defaulted)...
      { Sid = "DenyPutWithoutKmsEncryption", Effect = "Deny", Principal = "*", Action = "s3:PutObject", Resource = "${aws_s3_bucket.media.arn}/*", Condition = { StringNotEquals = { "s3:x-amz-server-side-encryption" = "aws:kms" } } },
      # ...with the project key, not some other KMS key.
      { Sid = "DenyPutWithOtherKmsKey", Effect = "Deny", Principal = "*", Action = "s3:PutObject", Resource = "${aws_s3_bucket.media.arn}/*", Condition = { StringNotEqualsIfExists = { "s3:x-amz-server-side-encryption-aws-kms-key-id" = aws_kms_key.media.arn } } },
    ]
  })
  depends_on = [aws_s3_bucket_public_access_block.media]
}

# ---------------------------------------------------------------- web bucket (private, served only through CloudFront OAC)
resource "aws_s3_bucket" "web" {
  bucket        = local.web_bucket
  force_destroy = true # static build artefacts only
}
resource "aws_s3_bucket_public_access_block" "web" {
  bucket                  = aws_s3_bucket.web.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}
resource "aws_s3_bucket_ownership_controls" "web" {
  bucket = aws_s3_bucket.web.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}
resource "aws_s3_bucket_server_side_encryption_configuration" "web" {
  bucket = aws_s3_bucket.web.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}
resource "aws_s3_bucket_policy" "web" {
  bucket = aws_s3_bucket.web.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Sid = "DenyInsecureTransport", Effect = "Deny", Principal = "*", Action = "s3:*", Resource = [aws_s3_bucket.web.arn, "${aws_s3_bucket.web.arn}/*"], Condition = { Bool = { "aws:SecureTransport" = "false" } } },
      { Sid = "AllowCloudFrontOAC", Effect = "Allow", Principal = { Service = "cloudfront.amazonaws.com" }, Action = "s3:GetObject", Resource = "${aws_s3_bucket.web.arn}/*",
      Condition = { StringEquals = { "AWS:SourceArn" = aws_cloudfront_distribution.main.arn } } },
    ]
  })
  depends_on = [aws_s3_bucket_public_access_block.web]
}

# ---------------------------------------------------------------- CloudTrail data events on the media bucket only
locals { trail_arn = "arn:aws:cloudtrail:${var.region}:${local.account_id}:trail/${local.name}-media-data" }

resource "aws_cloudtrail" "media_data" {
  name                          = "${local.name}-media-data"
  s3_bucket_name                = aws_s3_bucket.logs.id
  s3_key_prefix                 = "cloudtrail"
  include_global_service_events = false
  is_multi_region_trail         = false
  enable_log_file_validation    = true
  event_selector {
    read_write_type           = "All"
    include_management_events = false # data events only; management events are not recorded by this trail
    data_resource {
      type   = "AWS::S3::Object"
      values = ["${aws_s3_bucket.media.arn}/"]
    }
  }
  depends_on = [aws_s3_bucket_policy.logs]
}
