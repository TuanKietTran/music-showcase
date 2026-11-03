output "app_url" {
  description = "Where the web app and /api are served."
  value       = local.app_origin
}
output "cloudfront_domain" { value = aws_cloudfront_distribution.main.domain_name }
output "cloudfront_distribution_id" { value = aws_cloudfront_distribution.main.id }
output "alb_dns_name" { value = aws_lb.main.dns_name }
output "web_bucket" { value = aws_s3_bucket.web.id }
output "media_bucket" { value = aws_s3_bucket.media.id }
output "logs_bucket" { value = aws_s3_bucket.logs.id }
output "kms_key_arn" { value = aws_kms_key.media.arn }
output "ecr_repository_url" { value = aws_ecr_repository.server.repository_url }
output "ecs_cluster" { value = aws_ecs_cluster.main.name }
output "ecs_service" { value = aws_ecs_service.server.name }
output "user_pool_id" { value = aws_cognito_user_pool.main.id }
output "user_pool_client_id" { value = aws_cognito_user_pool_client.web.id }
output "cognito_hosted_domain" { value = "${aws_cognito_user_pool_domain.main.domain}.auth.${var.region}.amazoncognito.com" }
output "writer_role_arn" { value = aws_iam_role.writer.arn }
output "admin_token_parameter" { value = aws_ssm_parameter.admin_token.name }
output "admin_token" {
  description = "Bootstrap admin bearer token. Read with `terraform output -raw admin_token` into a 0600 file; never print it."
  value       = random_password.admin_token.result
  sensitive   = true
}
output "dns_records" {
  description = "Records to create at your DNS provider when domain_zone is set (all DNS-only / unproxied)."
  value = local.use_domain ? {
    acm_validation_cloudfront = [for o in aws_acm_certificate.cloudfront[0].domain_validation_options : { name = o.resource_record_name, type = o.resource_record_type, value = o.resource_record_value }]
    acm_validation_alb        = [for o in aws_acm_certificate.alb[0].domain_validation_options : { name = o.resource_record_name, type = o.resource_record_type, value = o.resource_record_value }]
    app_cname                 = { name = local.host, type = "CNAME", value = aws_cloudfront_distribution.main.domain_name }
    origin_cname              = { name = local.origin_host, type = "CNAME", value = aws_lb.main.dns_name }
  } : null
}
