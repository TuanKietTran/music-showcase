resource "aws_kms_key" "media" {
  description             = "Music Showcase media bucket (SSE-KMS)"
  enable_key_rotation     = true
  deletion_window_in_days = 7
}

resource "aws_kms_alias" "media" {
  name          = "alias/${local.name}-media"
  target_key_id = aws_kms_key.media.key_id
}
