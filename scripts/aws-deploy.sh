#!/usr/bin/env bash
# Deploy Music Showcase to AWS (Terraform in infra/).   usage: scripts/aws-deploy.sh plan | apply [--yes] | web | status
#   plan    terraform init + plan against the confirmed account. Creates nothing.
#   apply   build + push the image, terraform apply, build + upload the web client. Asks you to retype the account id unless --yes.
#   web     rebuild and upload only the web client, then invalidate CloudFront.
#   status  show endpoints, ECS service state and everything tagged app=music-showcase.
# Config lives outside the repo: ~/.music-showcase/deploy.tfvars (see infra/terraform.tfvars.example). No secrets are printed.
source "$(dirname "${BASH_SOURCE[0]}")/aws-lib.sh"
cmd="${1:-}"; shift || true
need aws; need terraform
load_conf; verify_account
tf init -input=false >/dev/null
TFARGS=(-input=false -var-file="$CONF")

image_tag() { local s; s="$(git -C "$ROOT" rev-parse --short=12 HEAD)"; git -C "$ROOT" diff --quiet HEAD -- server Dockerfile || s="$s-dirty$(date +%s)"; echo "$s"; }

print_dns() { # <keys...>: show records from the terraform output `dns_records`
  tf output -json dns_records | python3 -c '
import json, sys
d = json.load(sys.stdin) or {}
for k in sys.argv[1:]:
    v = d.get(k) or []
    for r in (v if isinstance(v, list) else [v]):
        print("  %-5s %s  ->  %s" % (r["type"], r["name"], r["value"]))' "$@"
}

case "$cmd" in
  plan)
    tf plan "${TFARGS[@]}" -out="$STATE_DIR/plan.tfplan"
    echo; echo "Plan saved to $STATE_DIR/plan.tfplan. Nothing has been created."
    ;;
  apply)
    need docker; need node; need npm
    if [ "${1:-}" != "--yes" ]; then read -r -p "Type the account id ($ACCOUNT_ID) to apply: " ans; [ "$ans" = "$ACCOUNT_ID" ] || die "account id not confirmed"; fi
    TAG="$(image_tag)"
    echo "== 1/5 registry (and certificates in domain mode)"
    targets=(-target=aws_ecr_repository.server -target=aws_ecr_lifecycle_policy.server)
    [ -n "$DOMAIN_ZONE" ] && targets+=(-target=aws_acm_certificate.cloudfront -target=aws_acm_certificate.alb)
    tf apply "${TFARGS[@]}" -auto-approve "${targets[@]}"
    if [ -n "$DOMAIN_ZONE" ]; then
      echo "Create these ACM validation records at your DNS provider now; the full apply waits (up to 45 min) for issuance:"
      print_dns acm_validation_cloudfront acm_validation_alb
    fi
    echo "== 2/5 image $TAG (linux/arm64)"
    REPO="$(tf_out ecr_repository_url)"
    aws ecr get-login-password | docker login --username AWS --password-stdin "${REPO%%/*}" >/dev/null
    docker buildx build --platform linux/arm64 --provenance=false --push -t "$REPO:$TAG" "$ROOT"
    echo "== 3/5 terraform apply"
    tf apply "${TFARGS[@]}" -var "image_tag=$TAG" -auto-approve
    echo "== 4/5 admin token -> $STATE_DIR/admin.token (0600, not printed)"
    tf output -raw admin_token | write_secret "$STATE_DIR/admin.token"
    echo "== 5/5 web client"
    "$0" web
    {
      echo "APP_URL=$(tf_out app_url)"; echo "REGION=$REGION"; echo "USER_POOL_ID=$(tf_out user_pool_id)"; echo "CLIENT_ID=$(tf_out user_pool_client_id)"
      echo "MEDIA_BUCKET=$(tf_out media_bucket)"; echo "CLOUDFRONT_DOMAIN=$(tf_out cloudfront_domain)"; echo "ALB_DNS=$(tf_out alb_dns_name)"
    } > "$STATE_DIR/endpoints.env"
    echo; echo "Done. App: $(tf_out app_url)   (details: $STATE_DIR/endpoints.env)"
    if [ -n "$DOMAIN_ZONE" ]; then echo "Create these CNAMEs (DNS-only):"; print_dns app_cname origin_cname; fi
    ;;
  web)
    need node; need npm
    BUCKET="$(tf_out web_bucket)"; DIST="$(tf_out cloudfront_distribution_id)"
    ( cd "$ROOT/web" && npm ci --no-audit --no-fund >/dev/null && npm run build >/dev/null )
    aws s3 sync "$ROOT/web/dist" "s3://$BUCKET" --delete --exclude index.html --cache-control "public,max-age=31536000,immutable" >/dev/null
    aws s3 cp "$ROOT/web/dist/index.html" "s3://$BUCKET/index.html" --cache-control "no-cache" --content-type "text/html; charset=utf-8" >/dev/null
    aws cloudfront create-invalidation --distribution-id "$DIST" --paths '/*' --query Invalidation.Id --output text >/dev/null
    echo "web client uploaded to s3://$BUCKET and CloudFront invalidated"
    ;;
  status)
    echo "app url: $(tf_out app_url)"
    aws ecs describe-services --cluster "$(tf_out ecs_cluster)" --services "$(tf_out ecs_service)" --query 'services[0].[status,desiredCount,runningCount,deployments[0].rolloutState]' --output text
    echo "-- tagged resources (app=music-showcase)"
    for r in "$REGION" us-east-1; do aws resourcegroupstaggingapi get-resources --region "$r" --tag-filters Key=app,Values=music-showcase --query 'ResourceTagMappingList[].ResourceARN' --output text | tr '\t' '\n'; done | sort -u | sed '/^$/d'
    ;;
  *) sed -n '2,7p' "$0"; exit 2 ;;
esac
