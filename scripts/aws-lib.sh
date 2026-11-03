# Shared helpers for the aws-*.sh scripts. Sourced, not executed.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INFRA="$ROOT/infra"
STATE_DIR="${MUSIC_AWS_DIR:-$HOME/.music-showcase}"      # outside the repo; holds config + secrets (0600)
CONF="${MUSIC_AWS_CONF:-$STATE_DIR/deploy.tfvars}"       # region, expected_account_id, budget_usd, alert_email, domain_zone, ...
umask 077; mkdir -p "$STATE_DIR"

die() { echo "error: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required"; }
tfvar() { sed -n "s/^[[:space:]]*$1[[:space:]]*=[[:space:]]*\"\{0,1\}\([^\"#]*[^\"# ]\)\"\{0,1\}[[:space:]]*\(#.*\)\{0,1\}$/\1/p" "$CONF" | head -1; }

load_conf() {
  [ -f "$CONF" ] || die "missing $CONF (copy infra/terraform.tfvars.example there and fill it in)"
  REGION="$(tfvar region)"; EXPECTED="$(tfvar expected_account_id)"; DOMAIN_ZONE="$(tfvar domain_zone)"
  [ -n "$REGION" ] && [ -n "$EXPECTED" ] || die "region and expected_account_id must be set in $CONF"
  export AWS_REGION="$REGION" AWS_DEFAULT_REGION="$REGION" AWS_PAGER=""
}

# Show and verify the caller. Never creates anything.
verify_account() {
  local who acct arn
  who="$(aws sts get-caller-identity --query '[Account,Arn]' --output text)" || die "aws sts get-caller-identity failed (not logged in?)"
  acct="${who%%[[:space:]]*}"; arn="${who#*[[:space:]]}"
  echo "AWS caller: account $acct  ($arn)  region $REGION"
  [ "$acct" = "$EXPECTED" ] || die "caller account $acct does not match expected_account_id in $CONF"
  ACCOUNT_ID="$acct"
}

tf() { terraform -chdir="$INFRA" "$@"; }
tf_out() { tf output -raw "$1"; }

# Write a secret to a 0600 file without printing it or putting it in argv.
write_secret() { local f="$1"; ( umask 077; cat > "$f" ); chmod 600 "$f"; }
