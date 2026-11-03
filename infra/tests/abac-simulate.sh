#!/usr/bin/env bash
# Live proof (read-only API calls) that the deployed writer role honours the accountId session tag:
# `aws iam simulate-principal-policy` against the real role and bucket.   usage: infra/tests/abac-simulate.sh   (run after apply)
set -euo pipefail
cd "$(dirname "$0")/.."
ROLE=$(terraform output -raw writer_role_arn); BUCKET=$(terraform output -raw media_bucket)
sim() { # <tag value or ""> <key> <action>
  local ctx=(); [ -n "$1" ] && ctx=(--context-entries "ContextKeyName=aws:PrincipalTag/accountId,ContextKeyValues=$1,ContextKeyType=string")
  aws iam simulate-principal-policy --policy-source-arn "$ROLE" --action-names "$3" --resource-arns "arn:aws:s3:::$BUCKET/$2" "${ctx[@]}" \
    --query 'EvaluationResults[0].EvalDecision' --output text
}
fail=0
expect() { local got; got=$(sim "$2" "$3" "$4"); if [ "$got" = "$1" ]; then echo "PASS $1: tag='$2' $4 $3"; else echo "FAIL expected $1 got $got: tag='$2' $4 $3"; fail=1; fi; }
expect allowed      alice accounts/alice/x.mp3 s3:PutObject
expect allowed      alice accounts/alice/x.mp3 s3:DeleteObject
expect implicitDeny alice accounts/bob/x.mp3   s3:PutObject     # mismatching tag
expect implicitDeny alice accounts/bob/x.mp3   s3:DeleteObject
expect explicitDeny ""    accounts/alice/x.mp3 s3:PutObject     # no tag at all
expect implicitDeny alice accounts/alice/x.mp3 s3:GetObject     # reads are not the writer's job
exit $fail
