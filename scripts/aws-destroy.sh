#!/usr/bin/env bash
# Tear the stack down.   usage: scripts/aws-destroy.sh --account-id <12 digits> [--purge-local]
# Refuses to run unless the calling account, the account id you pass, and expected_account_id in the config all agree.
# Versioned buckets are emptied first (every version and delete marker, plus stray multipart uploads), then terraform destroy runs.
source "$(dirname "${BASH_SOURCE[0]}")/aws-lib.sh"
need aws; need terraform; need jq
CONFIRM=""; PURGE=0
while [ $# -gt 0 ]; do case "$1" in --account-id) CONFIRM="${2:-}"; shift 2;; --purge-local) PURGE=1; shift;; *) die "unknown argument $1";; esac; done
[ -n "$CONFIRM" ] || die "pass --account-id <the account id you confirmed>"
load_conf; verify_account
[ "$CONFIRM" = "$ACCOUNT_ID" ] || die "--account-id $CONFIRM does not match the calling account $ACCOUNT_ID; refusing"

empty_versioned() { # <bucket>
  local b="$1" n list
  aws s3api head-bucket --bucket "$b" 2>/dev/null || { echo "bucket $b already gone"; return 0; }
  echo "emptying s3://$b (all versions and delete markers)"
  while :; do
    list="$(aws s3api list-object-versions --bucket "$b" --max-items 1000 --output json)"
    n="$(echo "$list" | jq '[(.Versions // []), (.DeleteMarkers // [])] | add | length')"
    [ "$n" -gt 0 ] || break
    echo "$list" | jq -c '{Objects: ([(.Versions // [])[], (.DeleteMarkers // [])[]] | map({Key, VersionId})), Quiet: true}' > "${STATE_DIR:?}/.del.json"
    aws s3api delete-objects --bucket "$b" --delete "file://${STATE_DIR:?}/.del.json" >/dev/null
  done
  rm -f "${STATE_DIR:?}/.del.json"
  aws s3api list-multipart-uploads --bucket "$b" --query 'Uploads[].[Key,UploadId]' --output text 2>/dev/null | while read -r key uid; do
    if [ -n "${key:-}" ] && [ "$key" != None ]; then aws s3api abort-multipart-upload --bucket "$b" --key "$key" --upload-id "$uid"; fi
  done || true
}

if tf output -raw media_bucket >/dev/null 2>&1; then
  for out in media_bucket logs_bucket; do empty_versioned "$(tf_out $out)"; done
else echo "no terraform outputs found (nothing deployed from this state?)"; fi

tf init -input=false >/dev/null
tf destroy -input=false -var-file="$CONF" -auto-approve

echo; echo "== resources still tagged app=music-showcase"
left=0
for r in "$REGION" us-east-1; do
  out="$(aws resourcegroupstaggingapi get-resources --region "$r" --tag-filters Key=app,Values=music-showcase --query 'ResourceTagMappingList[].ResourceARN' --output text | tr '\t' '\n' | sed '/^$/d')"
  if [ -n "$out" ]; then echo "[$r]"; echo "$out"; left=1; fi
done
[ $left -eq 0 ] && echo "(none)"
if [ $PURGE -eq 1 ]; then
  rm -f "${STATE_DIR:?}/admin.token" "${STATE_DIR:?}/endpoints.env" "${STATE_DIR:?}/plan.tfplan"
  echo "removed the admin token, endpoints and saved plan from $STATE_DIR"
fi
exit 0
