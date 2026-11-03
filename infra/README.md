# Infrastructure (Terraform)

Deploys the Music Showcase server and everything around it into **one account and one region**, tagged `app=music-showcase`, `env=dev`. Architecture, cost and runbook: `docs/aws.md`.

## What it creates

CloudFront (web via OAC + `/api/*` to the ALB), private web bucket, ALB (security group allows only CloudFront's origin-facing prefix list, on a single port), ECS Fargate service (ARM64, distroless image from ECR), media bucket (versioned, SSE-KMS CMK, access logging, lifecycle, CORS, TLS/encryption-enforcing policy), logs bucket, CloudTrail data events for the media bucket, DynamoDB `music-uploads`, Cognito user pool + groups + public PKCE client, three IAM roles (task, S3 writer with the `accountId` session tag, ECS execution), SSM SecureString for the bootstrap token, SNS + three CloudWatch alarms, an AWS Budget, optional WAF, and (with a domain) two ACM certificates.

## Use

```
mkdir -p ~/.music-showcase && cp infra/terraform.tfvars.example ~/.music-showcase/deploy.tfvars   # fill it in; stays outside git
scripts/aws-deploy.sh plan        # verifies the caller account, creates nothing
scripts/aws-deploy.sh apply       # image build + push, terraform apply, web upload
scripts/aws-verify.sh             # end-to-end proof with real calls
scripts/aws-destroy.sh --account-id <id> [--purge-local]
```

Variables have **no defaults** for `region`, `expected_account_id`, `budget_usd` and `alert_email`: you choose them. `expected_account_id` is wired into both AWS providers (`allowed_account_ids`), so Terraform refuses any other account. Account ids and emails live only in `~/.music-showcase/deploy.tfvars`.

### Custom domain (DNS stays outside Terraform)

Set `domain_zone = "example.com"` (and optionally `subdomain`). Nothing here holds DNS credentials, so you create the records yourself at your DNS provider, **DNS-only (unproxied)**:

1. During `apply`, the two ACM validation CNAMEs are printed (also `terraform output dns_records`). Create them; the apply waits up to 45 minutes for issuance.
2. After apply, create `music.example.com -> <cloudfront domain>` and `origin-music.example.com -> <alb dns name>`.

Without a domain the CloudFront URL is used and the CloudFront-to-ALB hop is plain HTTP (dev smoke test only).

## Remote state

State is **local** (`infra/terraform.tfstate`, git-ignored) and this configuration does **not** create a remote backend. The state contains the generated bootstrap token and origin-verify secret, so keep it private (it is `0600` by default via the umask in the scripts, never commit it). For shared use, create a private, versioned, encrypted S3 bucket and a lock mechanism yourself, then add a `backend "s3"` block and run `terraform init -migrate-state`. Ask before adding that to this repo.

## Tests

- `python3 infra/tests/policy_lint.py` - offline checks of the IAM policy templates (mismatching tag denied, no wildcard Allow actions).
- `infra/tests/abac-simulate.sh` - live `iam simulate-principal-policy` against the deployed writer role.
- `terraform -chdir=infra validate`.

## Notes

- The Budget is account-wide: cost allocation tags are not activated, so it cannot be filtered to this app.
- The KMS key is scheduled for deletion on destroy (7-day minimum wait); it may still appear in the tagging API as pending deletion for that period.
- CloudFront takes 10-20 minutes to create and to delete.
