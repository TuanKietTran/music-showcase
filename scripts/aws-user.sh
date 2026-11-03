#!/usr/bin/env bash
# Manage Cognito accounts for the deployed stack (admin-only API calls with your own AWS credentials; no secrets are printed).
#   scripts/aws-user.sh create  <email> <admin|uploader|listener> [quota_bytes]   random password -> ~/.music-showcase/users/<email>.pw (0600)
#   scripts/aws-user.sh set     <email> <role> [status active|suspended] [quota_bytes]
#   scripts/aws-user.sh revoke  <email>      disable + sign out everywhere (existing ID tokens live up to 15 min; see docs/aws.md for MUSIC_DENY_SUBJECTS)
#   scripts/aws-user.sh token   <email>      print nothing; writes an ID token to ~/.music-showcase/users/<email>.token (0600) for curl -H @file
source "$(dirname "${BASH_SOURCE[0]}")/aws-lib.sh"
need aws; need openssl; need jq
sub="${1:-}"; email="${2:-}"; [ -n "$sub" ] && [ -n "$email" ] || { sed -n '2,7p' "$0"; exit 2; }
load_conf; verify_account >/dev/null
POOL="$(tf_out user_pool_id)"; CLIENT="$(tf_out user_pool_client_id)"
USERS="${STATE_DIR:?}/users"; mkdir -p "$USERS"; chmod 700 "$USERS"
json_tmp() { local f; f="$(mktemp "$USERS/.req.XXXXXX")"; chmod 600 "$f"; cat > "$f"; echo "$f"; }
case "$sub" in
  create)
    role="${3:-}"; quota="${4:-268435456}"; case "$role" in admin|uploader|listener) ;; *) die "role must be admin|uploader|listener";; esac
    pw="$(openssl rand -base64 24 | tr -d '/+=' | cut -c1-20)Aa1!"
    aws cognito-idp admin-create-user --user-pool-id "$POOL" --username "$email" --message-action SUPPRESS \
      --user-attributes "Name=email,Value=$email" "Name=email_verified,Value=true" "Name=custom:role,Value=$role" "Name=custom:status,Value=active" "Name=custom:quota_bytes,Value=$quota" >/dev/null
    req="$(jq -n --arg p "$POOL" --arg u "$email" --arg w "$pw" '{UserPoolId:$p,Username:$u,Password:$w,Permanent:true}' | json_tmp)"
    aws cognito-idp admin-set-user-password --cli-input-json "file://$req" >/dev/null; rm -f "$req"
    aws cognito-idp admin-add-user-to-group --user-pool-id "$POOL" --username "$email" --group-name "$role" >/dev/null
    printf '%s' "$pw" | write_secret "$USERS/$email.pw"
    echo "created $email ($role, quota $quota); password stored in $USERS/$email.pw"
    ;;
  set)
    role="${3:-}"; status="${4:-active}"; attrs=("Name=custom:status,Value=$status")
    [ -n "$role" ] && attrs+=("Name=custom:role,Value=$role"); [ -n "${5:-}" ] && attrs+=("Name=custom:quota_bytes,Value=$5")
    aws cognito-idp admin-update-user-attributes --user-pool-id "$POOL" --username "$email" --user-attributes "${attrs[@]}" >/dev/null
    echo "updated $email (takes effect at the user's next token refresh, at most 15 minutes)"
    ;;
  revoke)
    aws cognito-idp admin-disable-user --user-pool-id "$POOL" --username "$email" >/dev/null
    aws cognito-idp admin-user-global-sign-out --user-pool-id "$POOL" --username "$email" >/dev/null
    echo "disabled $email and revoked refresh tokens. ID tokens already issued stay valid until they expire (<= 15 min);"
    echo "for an immediate cut-off add the subject id to deny_subjects and apply:"
    aws cognito-idp admin-get-user --user-pool-id "$POOL" --username "$email" --query "UserAttributes[?Name=='sub'].Value" --output text
    ;;
  token)
    [ -f "$USERS/$email.pw" ] || die "no stored password for $email"
    req="$(jq -n --arg p "$POOL" --arg c "$CLIENT" --arg u "$email" --rawfile w "$USERS/$email.pw" '{UserPoolId:$p,ClientId:$c,AuthFlow:"ADMIN_USER_PASSWORD_AUTH",AuthParameters:{USERNAME:$u,PASSWORD:$w}}' | json_tmp)"
    aws cognito-idp admin-initiate-auth --cli-input-json "file://$req" --query 'AuthenticationResult.IdToken' --output text | { read -r t; printf 'Authorization: Bearer %s\n' "$t" | write_secret "$USERS/$email.token"; }
    rm -f "$req"; echo "wrote an Authorization header file: $USERS/$email.token (use: curl -H @file)"
    ;;
  *) sed -n '2,7p' "$0"; exit 2 ;;
esac
