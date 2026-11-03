# Three roles, nothing else in IAM:
#   task   - the server's identity: reads accounts/*, uses the upload index table, may assume `writer` WITH an accountId session tag.
#   writer - writes/deletes only under accounts/${aws:PrincipalTag/accountId}/* (the AWS-side ABAC; see docs/aws.md).
#   exec   - lets ECS pull the image, write logs and inject the token secret.
data "aws_iam_policy_document" "ecs_tasks_trust" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }
  }
}

resource "aws_iam_role" "task" {
  name               = "${local.name}-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_trust.json
}

resource "aws_iam_role" "writer" {
  name                 = "${local.name}-s3-writer"
  assume_role_policy   = templatefile("${path.module}/policies/writer-trust.json.tftpl", { task_role_arn = aws_iam_role.task.arn })
  max_session_duration = 3600
}

resource "aws_iam_role_policy" "task" {
  name = "server"
  role = aws_iam_role.task.id
  policy = templatefile("${path.module}/policies/task-role.json.tftpl", {
    bucket_arn      = aws_s3_bucket.media.arn, kms_arn = aws_kms_key.media.arn, region = var.region,
    writer_role_arn = aws_iam_role.writer.arn, table_arn = aws_dynamodb_table.uploads.arn
  })
}

resource "aws_iam_role_policy" "writer" {
  name   = "own-prefix-writes"
  role   = aws_iam_role.writer.id
  policy = templatefile("${path.module}/policies/writer-role.json.tftpl", { bucket_arn = aws_s3_bucket.media.arn, kms_arn = aws_kms_key.media.arn, region = var.region })
}

resource "aws_iam_role" "exec" {
  name               = "${local.name}-exec"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_trust.json
}
resource "aws_iam_role_policy_attachment" "exec_managed" {
  role       = aws_iam_role.exec.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy" # AWS-managed; contains the wildcards AWS requires for ECR auth and logs
}
resource "aws_iam_role_policy" "exec_secret" {
  name = "read-token-parameter"
  role = aws_iam_role.exec.id
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "ssm:GetParameters", Resource = aws_ssm_parameter.admin_token.arn }]
  })
}
