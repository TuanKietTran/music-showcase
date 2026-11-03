#!/usr/bin/env bash
# End-to-end proof against the deployed stack (docs/aws.md "Verification"). Prints what it did and the HTTP results; never prints tokens.
#   scripts/aws-verify.sh        needs: a finished scripts/aws-deploy.sh apply, aws cli, curl, jq, openssl
source "$(dirname "${BASH_SOURCE[0]}")/aws-lib.sh"
need aws; need curl; need jq; need openssl
load_conf; verify_account
set -a; source "$STATE_DIR/endpoints.env"; set +a
U="$STATE_DIR/users"; W="$(mktemp -d "$STATE_DIR/.verify.XXXXXX")"; trap 'rm -rf "${W:?}"' EXIT
pass=0; failn=0
check() { # <desc> <expected> <actual>
  if [ "$2" = "$3" ]; then echo "PASS  $1  [$3]"; pass=$((pass+1)); else echo "FAIL  $1  expected [$2] got [$3]"; failn=$((failn+1)); fi
}
S=$RANDOM; ALICE="verify-alice-$S@music.invalid"; BOB="verify-bob-$S@music.invalid"; LISA="verify-lisa-$S@music.invalid"
echo "== accounts: alice(uploader, quota 300000 B), bob(uploader), lisa(listener)"
"$ROOT/scripts/aws-user.sh" create "$ALICE" uploader 300000; "$ROOT/scripts/aws-user.sh" create "$BOB" uploader 300000; "$ROOT/scripts/aws-user.sh" create "$LISA" listener 0
for u in "$ALICE" "$BOB" "$LISA"; do "$ROOT/scripts/aws-user.sh" token "$u" >/dev/null; done
# synthetic MP3: ID3v2 header + random payload (the server validates magic bytes, not decodability)
mk() { { printf 'ID3\x04\x00\x00\x00\x00\x00\x00'; head -c $(($1-10)) /dev/urandom; } > "$2"; }
mk 200000 "$W/a.mp3"; mk 120000 "$W/b.mp3"
req() { # <tokenfile|-> <method> <path> [curl args...] -> prints http code; body in $W/body
  local tf="$1" m="$2" p="$3"; shift 3; local h=(); [ "$tf" != "-" ] && h=(-H "@$tf")
  curl -sS -o "$W/body" -D "$W/hdr" -w '%{http_code}' -X "$m" "${h[@]}" "$@" "$APP_URL$p"
}
TA="$U/$ALICE.token"; TB="$U/$BOB.token"; TL="$U/$LISA.token"
echo; echo "== API through CloudFront ($APP_URL)"
check "no token -> 401" 401 "$(req - GET /api/tracks)"
check "health (no auth) -> 200" 200 "$(req - GET /api/health)"
check "alice uploads mp3 (private) -> 201" 201 "$(req "$TA" POST '/api/tracks?title=Verify%20Song&artist=Alice&visibility=private' -H 'Content-Type: audio/evil' --data-binary @"$W/a.mp3")"
ID="$(jq -r .id "$W/body")"; OWNER="$(jq -r .ownerId "$W/body")"; echo "      track id $ID owner(sub) $OWNER bytes $(jq -r .bytes "$W/body")"
check "alice streams with Range -> 206" 206 "$(req "$TA" GET "/api/tracks/$ID/stream" -H 'Range: bytes=100-199')"
echo "      Content-Range: $(grep -i '^content-range' "$W/hdr" | tr -d '\r') ; body bytes: $(wc -c < "$W/body" | tr -d ' ')"
check "ranged body is the right 100 bytes" "$(tail -c +101 "$W/a.mp3" | head -c 100 | shasum | cut -d' ' -f1)" "$(shasum < "$W/body" | cut -d' ' -f1)"
check "bob (other account) cannot read alice's private track -> 404" 404 "$(req "$TB" GET "/api/tracks/$ID/stream")"
check "bob does not see it in the list" "0" "$(req "$TB" GET /api/tracks >/dev/null; jq "[.[]|select(.id==\"$ID\")]|length" "$W/body")"
check "lisa (listener) cannot upload -> 403" 403 "$(req "$TL" POST /api/tracks --data-binary @"$W/b.mp3")"
check "alice 2nd upload exceeds quota (200000+120000>300000) -> 403" 403 "$(req "$TA" POST '/api/tracks?title=Too%20Big' --data-binary @"$W/b.mp3")"
check "  ...with error quota_exceeded" quota_exceeded "$(jq -r .error "$W/body")"
check "unsupported media -> 415" 415 "$(req "$TA" POST /api/tracks --data-binary 'plain text, not audio')"

echo; echo "== S3 key and encryption"
KEY="accounts/$OWNER/$ID.mp3"
aws s3api head-object --bucket "$MEDIA_BUCKET" --key "$KEY" --query '[ContentType,ServerSideEncryption,SSEKMSKeyId!=null,ContentLength]' --output text | tr '\t' ' ' | { read -r ct sse haskey len; check "object at contract key, Content-Type from sniffed format" "audio/mpeg" "$ct"; check "SSE-KMS" "aws:kms" "$sse"; check "stored size" "200000" "$len"; }

echo; echo "== bucket policy, called directly with the account's own credentials"
echo hello > "$W/plain.txt"
out="$(aws s3api put-object --bucket "$MEDIA_BUCKET" --key accounts/verify-direct/0123456789abcdef.mp3 --body "$W/plain.txt" 2>&1 || true)"
echo "      $(echo "$out" | head -1 | cut -c1-200)"
check "put WITHOUT encryption header is denied" 1 "$(echo "$out" | grep -c 'AccessDenied')"
out="$(aws s3api put-object --bucket "$MEDIA_BUCKET" --key accounts/verify-direct/0123456789abcdef.mp3 --body "$W/plain.txt" --server-side-encryption AES256 2>&1 || true)"
check "put with SSE-S3 (wrong algorithm) is denied" 1 "$(echo "$out" | grep -c 'AccessDenied')"
out="$(aws s3api head-object --bucket "$MEDIA_BUCKET" --key "$KEY" --endpoint-url "http://s3.$REGION.amazonaws.com" 2>&1 || true)"
echo "      $(echo "$out" | head -1 | cut -c1-200)"
check "request over plain HTTP (no TLS) is denied" 1 "$(echo "$out" | grep -c 'AccessDenied\|Forbidden\|403')"
check "positive control: SSE-KMS put with the project key succeeds" 0 "$(aws s3api put-object --bucket "$MEDIA_BUCKET" --key accounts/verify-direct/0123456789abcdef.mp3 --body "$W/plain.txt" --server-side-encryption aws:kms --ssekms-key-id "$(tf_out kms_key_arn)" >/dev/null 2>&1; echo $?)"
aws s3api delete-object --bucket "$MEDIA_BUCKET" --key accounts/verify-direct/0123456789abcdef.mp3 >/dev/null

echo; echo "== delete removes the current object"
check "alice deletes her track -> 204" 204 "$(req "$TA" DELETE "/api/tracks/$ID")"
check "stream after delete -> 404" 404 "$(req "$TA" GET "/api/tracks/$ID/stream")"
out="$(aws s3api head-object --bucket "$MEDIA_BUCKET" --key "$KEY" 2>&1 || true)"
check "head-object on the key now fails (delete marker is the current version)" 1 "$(echo "$out" | grep -c '404\|Not Found')"
echo "      versions kept for restore: $(aws s3api list-object-versions --bucket "$MEDIA_BUCKET" --prefix "$KEY" --query 'length(Versions)' --output text) version(s), delete marker: $(aws s3api list-object-versions --bucket "$MEDIA_BUCKET" --prefix "$KEY" --query 'length(DeleteMarkers)' --output text)"

echo; echo "== budget, alarms, trail"
aws budgets describe-budget --account-id "$ACCOUNT_ID" --budget-name music-showcase-dev-monthly --query 'Budget.[BudgetName,BudgetLimit.Amount,BudgetLimit.Unit,TimeUnit]' --output text
aws cloudwatch describe-alarms --alarm-name-prefix music-showcase --query 'MetricAlarms[].[AlarmName,StateValue]' --output text
aws cloudtrail get-event-selectors --trail-name music-showcase-media-data --query 'EventSelectors[].[IncludeManagementEvents,DataResources[0].Type,DataResources[0].Values[0]]' --output text
check "budget exists" music-showcase-dev-monthly "$(aws budgets describe-budget --account-id "$ACCOUNT_ID" --budget-name music-showcase-dev-monthly --query Budget.BudgetName --output text)"
check "3 alarms exist" 3 "$(aws cloudwatch describe-alarms --alarm-name-prefix music-showcase --query 'length(MetricAlarms)' --output text)"

echo; echo "== AWS-side ABAC (IAM simulation)"; "$INFRA/tests/abac-simulate.sh" || failn=$((failn+1))
echo; echo "summary: $pass passed, $failn failed"
echo "cleanup: aws-user.sh revoke/delete the three verify-* users is done by aws-destroy.sh (the pool is deleted with the stack)"
[ "$failn" -eq 0 ]
