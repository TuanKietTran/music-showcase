resource "aws_ecr_repository" "server" {
  name                 = "${local.name}-server"
  image_tag_mutability = "MUTABLE"
  force_delete         = true # destroy removes the images too
  image_scanning_configuration { scan_on_push = true }
  encryption_configuration { encryption_type = "AES256" }
}
resource "aws_ecr_lifecycle_policy" "server" {
  repository = aws_ecr_repository.server.name
  policy     = jsonencode({ rules = [{ rulePriority = 1, description = "keep the last 5 images", selection = { tagStatus = "any", countType = "imageCountMoreThan", countNumber = 5 }, action = { type = "expire" } }] })
}

resource "aws_cloudwatch_log_group" "server" {
  name              = "/ecs/${local.name}"
  retention_in_days = 14
}

# ---------------------------------------------------------------- network (default VPC, public subnets, no NAT gateway)
data "aws_ec2_managed_prefix_list" "cloudfront" { name = "com.amazonaws.global.cloudfront.origin-facing" }

resource "aws_security_group" "alb" {
  name        = "${local.name}-alb"
  description = "ALB: only CloudFront origin-facing addresses, one port"
  vpc_id      = data.aws_vpc.default.id
}
# Exactly one of 80/443 is open (443 with a custom domain, 80 for the clearly marked dev smoke test), and only to CloudFront.
resource "aws_vpc_security_group_ingress_rule" "alb_from_cloudfront" {
  security_group_id = aws_security_group.alb.id
  prefix_list_id    = data.aws_ec2_managed_prefix_list.cloudfront.id
  ip_protocol       = "tcp"
  from_port         = local.use_domain ? 443 : 80
  to_port           = local.use_domain ? 443 : 80
  description       = "CloudFront origin-facing"
}
resource "aws_vpc_security_group_egress_rule" "alb_to_task" {
  security_group_id            = aws_security_group.alb.id
  referenced_security_group_id = aws_security_group.task.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}

resource "aws_security_group" "task" {
  name        = "${local.name}-task"
  description = "Fargate task: only the ALB may connect"
  vpc_id      = data.aws_vpc.default.id
}
resource "aws_vpc_security_group_ingress_rule" "task_from_alb" {
  security_group_id            = aws_security_group.task.id
  referenced_security_group_id = aws_security_group.alb.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}
resource "aws_vpc_security_group_egress_rule" "task_https_out" {
  security_group_id = aws_security_group.task.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
  description       = "ECR, S3, STS, DynamoDB, SSM, CloudWatch Logs, Cognito JWKS (no NAT gateway: the task has a public IP, inbound is still closed)"
}

# ---------------------------------------------------------------- ALB
resource "aws_lb" "main" {
  name                       = "${local.name}-alb"
  load_balancer_type         = "application"
  internal                   = false
  security_groups            = [aws_security_group.alb.id]
  subnets                    = slice(sort(data.aws_subnets.default.ids), 0, 2) # two AZs are enough; every extra subnet is another billed public IPv4
  idle_timeout               = 300                                             # uploads and long range streams must not be cut by the default 60 s
  drop_invalid_header_fields = true
}

resource "aws_lb_target_group" "server" {
  name                 = "${local.name}-tg"
  port                 = 8080
  protocol             = "HTTP"
  target_type          = "ip"
  vpc_id               = data.aws_vpc.default.id
  deregistration_delay = 30
  health_check {
    path                = "/api/health"
    matcher             = "200"
    interval            = 15
    timeout             = 5
    healthy_threshold   = 2
    unhealthy_threshold = 3
  }
}

# Without a domain: HTTP:80 (dev smoke test only). With a domain: HTTPS:443 with an ACM certificate.
resource "aws_lb_listener" "main" {
  load_balancer_arn = aws_lb.main.arn
  port              = local.use_domain ? 443 : 80
  protocol          = local.use_domain ? "HTTPS" : "HTTP"
  ssl_policy        = local.use_domain ? "ELBSecurityPolicy-TLS13-1-2-2021-06" : null
  certificate_arn   = local.use_domain ? aws_acm_certificate_validation.alb[0].certificate_arn : null
  default_action {
    type = "fixed-response"
    fixed_response {
      content_type = "application/json"
      message_body = "{\"error\":\"forbidden\"}"
      status_code  = "403"
    }
  }
}
# Second layer behind the CloudFront-only security group: the shared secret header CloudFront adds.
resource "aws_lb_listener_rule" "from_cloudfront" {
  listener_arn = aws_lb_listener.main.arn
  priority     = 10
  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.server.arn
  }
  condition {
    http_header {
      http_header_name = "X-Origin-Verify"
      values           = [random_password.origin_verify.result]
    }
  }
}

# ---------------------------------------------------------------- ECS Fargate
resource "aws_ecs_cluster" "main" { name = local.name }

resource "aws_ecs_task_definition" "server" {
  family                   = "${local.name}-server"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.exec.arn
  task_role_arn            = aws_iam_role.task.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "ARM64"
  }
  container_definitions = jsonencode([{
    name            = "server"
    image           = "${aws_ecr_repository.server.repository_url}:${var.image_tag}"
    essential       = true
    portMappings    = [{ containerPort = 8080, protocol = "tcp" }]
    user            = "65532:65532"
    linuxParameters = { capabilities = { drop = ["ALL"] }, initProcessEnabled = false }
    environment = [
      { name = "MUSIC_BIND", value = "0.0.0.0" },
      { name = "MUSIC_PORT", value = "8080" },
      { name = "MUSIC_DATA_DIR", value = "/data" },
      { name = "MUSIC_ORIGIN", value = local.app_origin },
      { name = "MUSIC_TRUST_PROXY", value = "1" },
      { name = "MUSIC_MAX_UPLOAD_BYTES", value = tostring(var.max_upload_bytes) },
      { name = "MUSIC_DEFAULT_QUOTA_BYTES", value = tostring(var.default_quota_bytes) },
      { name = "MUSIC_STORAGE", value = "s3" },
      { name = "MUSIC_S3_BUCKET", value = aws_s3_bucket.media.id },
      { name = "MUSIC_S3_REGION", value = var.region },
      { name = "MUSIC_S3_KMS_KEY_ARN", value = aws_kms_key.media.arn },
      { name = "MUSIC_S3_WRITER_ROLE_ARN", value = aws_iam_role.writer.arn },
      { name = "MUSIC_DDB_TABLE", value = aws_dynamodb_table.uploads.name },
      { name = "MUSIC_COGNITO_REGION", value = var.region },
      { name = "MUSIC_COGNITO_USER_POOL_ID", value = aws_cognito_user_pool.main.id },
      { name = "MUSIC_COGNITO_CLIENT_ID", value = aws_cognito_user_pool_client.web.id },
      { name = "MUSIC_DENY_SUBJECTS", value = join(",", var.deny_subjects) },
    ]
    secrets          = [{ name = "MUSIC_TOKEN", valueFrom = aws_ssm_parameter.admin_token.arn }]
    healthCheck      = { command = ["CMD", "/music-server", "healthcheck"], interval = 15, timeout = 5, retries = 3, startPeriod = 20 }
    logConfiguration = { logDriver = "awslogs", options = { "awslogs-group" = aws_cloudwatch_log_group.server.name, "awslogs-region" = var.region, "awslogs-stream-prefix" = "server" } }
  }])
}

resource "aws_ecs_service" "server" {
  name                               = "server"
  cluster                            = aws_ecs_cluster.main.id
  task_definition                    = aws_ecs_task_definition.server.arn
  desired_count                      = var.desired_count
  launch_type                        = "FARGATE"
  deployment_minimum_healthy_percent = 0 # stop-then-start keeps a single writer of the in-memory index during deploys
  deployment_maximum_percent         = 100
  health_check_grace_period_seconds  = 30
  deployment_circuit_breaker {
    enable   = true
    rollback = false
  }
  network_configuration {
    subnets          = data.aws_subnets.default.ids
    security_groups  = [aws_security_group.task.id]
    assign_public_ip = true
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.server.arn
    container_name   = "server"
    container_port   = 8080
  }
  depends_on = [aws_lb_listener_rule.from_cloudfront, aws_iam_role_policy.task, aws_iam_role_policy_attachment.exec_managed]
}
