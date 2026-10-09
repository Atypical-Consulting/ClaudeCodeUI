#!/usr/bin/env bash
# Sets the 5 GitHub Actions secrets the Release workflow needs to sign and notarize the macOS app.
# Checks every input before uploading anything; never prints a secret.
#
#   scripts/set-apple-secrets.sh <DeveloperID.p12> <AuthKey_XXXXXXXXXX.p8> <issuer-id>
#
# - DeveloperID.p12: the "Developer ID Application" certificate WITH its private key, exported from
#   Keychain Access (right-click the certificate > Export). You are asked for its password.
# - AuthKey_XXXXXXXXXX.p8: App Store Connect > Users and Access > Integrations > Team Keys. The Key ID is
#   read from the file name; pass --key-id if you renamed it.
# - issuer-id: the UUID shown above the keys table on that same page.
#
# Run it in your own terminal (it prompts for the .p12 password).
set -euo pipefail

repo="Atypical-Consulting/ClaudeCodeUI"
key_id=""
args=()
while (($#)); do
  case "$1" in
    --key-id) key_id="$2"; shift 2 ;;
    --repo) repo="$2"; shift 2 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) args+=("$1"); shift ;;
  esac
done
((${#args[@]} == 3)) || { sed -n '2,15p' "$0" >&2; exit 2; }
p12="${args[0]}" p8="${args[1]}" issuer="${args[2]}"

fail() { echo "✗ $*" >&2; exit 1; }
ok() { echo "✓ $*"; }

command -v gh >/dev/null || fail "gh (GitHub CLI) is not installed"
command -v openssl >/dev/null || fail "openssl is not installed"
gh auth status >/dev/null 2>&1 || fail "gh is not logged in: run 'gh auth login'"
gh api "repos/$repo" --jq .permissions.admin 2>/dev/null | grep -qx true || fail "you need admin rights on $repo to set secrets"

# --- .p12 ------------------------------------------------------------------------------------------
[[ -f "$p12" ]] || fail "certificate not found: $p12"
# read fails at EOF without a trailing newline (password piped from a file): keep what it read.
read -rsp "Password of $(basename "$p12"): " P12_PASSWORD || [[ -n "${P12_PASSWORD:-}" ]] || fail "no password given"; echo
export P12_PASSWORD
pk12() {   # Keychain exports use legacy ciphers that OpenSSL 3 only reads with -legacy
  openssl pkcs12 -in "$p12" -passin env:P12_PASSWORD "$@" 2>/dev/null \
    || openssl pkcs12 -legacy -in "$p12" -passin env:P12_PASSWORD "$@" 2>/dev/null
}
subject=$(pk12 -nokeys -clcerts | openssl x509 -noout -subject 2>/dev/null) || fail "cannot open $p12: wrong password or not a .p12"
[[ "$subject" == *"Developer ID Application"* ]] \
  || fail "the certificate is not a 'Developer ID Application' one ($subject). 'Apple Development' or 'Mac App Distribution' cannot notarize."
pk12 -nocerts -nodes | grep -q "PRIVATE KEY" || fail "$p12 has no private key: export the certificate together with its key"
end=$(pk12 -nokeys -clcerts | openssl x509 -noout -enddate | cut -d= -f2)
ok "certificate: ${subject#*CN=} (valid until $end)"

# --- .p8 / key id / issuer ---------------------------------------------------------------------------
[[ -f "$p8" ]] || fail "API key not found: $p8"
head -1 "$p8" | grep -q "BEGIN PRIVATE KEY" || fail "$p8 is not an App Store Connect .p8 key"
if [[ -z "$key_id" ]]; then
  key_id=$(basename "$p8" | sed -n 's/^AuthKey_\([A-Z0-9]*\)\.p8$/\1/p')
  [[ -n "$key_id" ]] || fail "cannot read the Key ID from the file name; pass --key-id XXXXXXXXXX"
fi
[[ "$key_id" =~ ^[A-Z0-9]{10}$ ]] || fail "Key ID '$key_id' should be 10 uppercase letters/digits"
[[ "$issuer" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]] \
  || fail "issuer id '$issuer' should be a UUID"
ok "API key $key_id, issuer ${issuer:0:8}…"

# --- upload ------------------------------------------------------------------------------------------
base64 < "$p12" | tr -d '\n' | gh secret set APPLE_CERTIFICATE -R "$repo"
printf '%s' "$P12_PASSWORD" | gh secret set APPLE_CERTIFICATE_PASSWORD -R "$repo"
gh secret set APPLE_API_KEY_P8 -R "$repo" < "$p8"
printf '%s' "$key_id" | gh secret set APPLE_API_KEY -R "$repo"
printf '%s' "$issuer" | gh secret set APPLE_API_ISSUER -R "$repo"
unset P12_PASSWORD
ok "5 secrets set on $repo:"
gh secret list -R "$repo" | grep '^APPLE_' | cut -f1 | sed 's/^/    /'
echo
echo "Next: a signed + notarized dry run →  gh workflow run Release -R $repo"
