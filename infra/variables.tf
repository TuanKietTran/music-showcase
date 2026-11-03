variable "region" {
  description = "AWS region for all regional resources (no default on purpose: you choose)."
  type        = string
}

variable "expected_account_id" {
  description = "Account id you confirmed with `aws sts get-caller-identity`. Terraform refuses any other account."
  type        = string
  validation {
    condition     = can(regex("^[0-9]{12}$", var.expected_account_id))
    error_message = "expected_account_id must be a 12-digit account id."
  }
}

variable "budget_usd" {
  description = "Monthly cost budget in USD for the account-wide AWS Budget created by this stack."
  type        = number
}

variable "alert_email" {
  description = "Email for budget alerts and CloudWatch alarms (the SNS subscription must be confirmed from the inbox)."
  type        = string
}

variable "domain_zone" {
  description = "Optional DNS zone you control (e.g. example.com), hosted anywhere (Cloudflare is fine). Empty = no custom domain: CloudFront's own https URL is used and CloudFront reaches the ALB over plain HTTP (DEV SMOKE TEST ONLY)."
  type        = string
  default     = ""
}

variable "subdomain" {
  description = "Host label under domain_zone for the app; the ALB origin uses origin-<subdomain>."
  type        = string
  default     = "music"
}

variable "image_tag" {
  description = "Tag of the server image in the ECR repository (scripts/aws-deploy.sh sets this to the git sha)."
  type        = string
  default     = "latest"
}

variable "desired_count" {
  description = "Number of server tasks. Keep 1: the upload index is cached in memory by a single writer (docs/aws.md)."
  type        = number
  default     = 1
}

variable "task_cpu" {
  type    = number
  default = 256
}

variable "task_memory" {
  type    = number
  default = 512
}

variable "max_upload_bytes" {
  type    = number
  default = 104857600
}

variable "default_quota_bytes" {
  description = "Quota for Cognito users without a custom:quota_bytes claim."
  type        = number
  default     = 268435456
}

variable "enable_admin_auth_flow" {
  description = "Enable ALLOW_ADMIN_USER_PASSWORD_AUTH on the app client so operators/tests can mint tokens with IAM-signed calls. Set false for production."
  type        = bool
  default     = true
}

variable "enable_waf" {
  description = "Attach a minimal AWS WAFv2 web ACL (Common rule set + rate limit) to CloudFront. About USD 7/month plus 0.60 per million requests; off until you say yes."
  type        = bool
  default     = false
}

variable "deny_subjects" {
  description = "Emergency revocation: Cognito subject ids forced to `suspended` by the server (MUSIC_DENY_SUBJECTS) until their token expires. Apply to take effect (~2 min)."
  type        = list(string)
  default     = []
}
