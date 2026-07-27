#!/usr/bin/env bash
# Sync ChatGPT subscription OAuth material from Codex auth.json into Keryx secret files.
# Prefer this over Platform API keys when you want Plus/Pro plan usage via openai_codex.
set -euo pipefail

CODEX_AUTH="${CODEX_AUTH:-$HOME/.codex/auth.json}"
OUT_DIR="${KERYX_CONFIG_DIR:-$HOME/.config/keryx}"
mkdir -p "$OUT_DIR"

if [[ ! -f "$CODEX_AUTH" ]]; then
  echo "sync-chatgpt-codex-auth: missing $CODEX_AUTH — run: codex login" >&2
  exit 1
fi

python3 - "$CODEX_AUTH" "$OUT_DIR" <<'PY'
import json, sys, base64
from pathlib import Path

auth_path = Path(sys.argv[1])
out_dir = Path(sys.argv[2])
auth = json.loads(auth_path.read_text())
tokens = auth.get("tokens") or {}
access = tokens.get("access_token")
if not access:
    raise SystemExit(f"no tokens.access_token in {auth_path}")

account = tokens.get("account_id")
# Prefer claim from JWT
try:
    payload = access.split(".")[1]
    pad = "=" * ((4 - len(payload) % 4) % 4)
    raw = base64.urlsafe_b64decode(payload + pad)
    claims = json.loads(raw)
    account = (
        (claims.get("https://api.openai.com/auth") or {}).get("chatgpt_account_id")
        or account
    )
    plan = (claims.get("https://api.openai.com/auth") or {}).get("chatgpt_plan_type")
except Exception:
    plan = None

token_file = out_dir / "chatgpt-access-token"
token_file.write_text(access.strip() + "\n")
token_file.chmod(0o600)

if account:
    acct_file = out_dir / "chatgpt-account-id"
    acct_file.write_text(str(account).strip() + "\n")
    acct_file.chmod(0o600)

print(f"synced access_token len={len(access)} plan={plan!r} account_set={bool(account)}")
print(f"files: {token_file}" + (f", {out_dir / 'chatgpt-account-id'}" if account else ""))
PY
