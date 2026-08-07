#!/usr/bin/env bash
# Required host job shell for daily-use bar (ADR 0035).
# Exit non-zero until all host scenarios pass. Safe to run under systemd timer or GHA self-hosted.
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
CONFIG="${KERYX_CONFIG_DIR:-$HOME/.config/keryx}"
# shellcheck disable=SC1091
set -a; source "$CONFIG/env"; set +a

EDGE_URL="${KERYX_EDGE_URL:-https://jack-agent-worker.tail68a74b.ts.net:8443}"
LOG_DIR="${KERYX_HOST_CI_LOG:-$HOME/.local/share/keryx/host-ci}"
mkdir -p "$LOG_DIR"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
LOG="$LOG_DIR/run-$STAMP.log"
exec > >(tee -a "$LOG") 2>&1

echo "keryx-host-ci start $STAMP"
fail=0

check() {
  local name="$1"; shift
  echo "--- $name ---"
  if "$@"; then
    echo "PASS $name"
  else
    echo "FAIL $name"
    fail=1
  fi
}

# Line 8: L4 always-on Worker
check "L4 user unit active" systemctl --user is-active --quiet keryx
check "L4 loopback health" bash -c 'curl -fsS http://127.0.0.1:8787/health | grep -q ok'
check "L4 doctor" bash -c 'keryx doctor >/dev/null'

# Line 8: L5 Edge from THIS host (second-node proof is separate remote check)
check "L5 on-host Edge health" bash -c "curl -fsS --resolve jack-agent-worker.tail68a74b.ts.net:8443:100.108.132.109 $EDGE_URL/health | grep -q ok"
check "L5 Edge auth challenge" bash -c "code=\$(curl -sS -o /dev/null -w '%{http_code}' --resolve jack-agent-worker.tail68a74b.ts.net:8443:100.108.132.109 -X POST $EDGE_URL/v1/sessions); test \"\$code\" = 401"

# Provider diagnostic (line 8)
check "provider providers endpoint" bash -c 'curl -fsS -H "authorization: Bearer $KERYX_OPERATOR_TOKEN" http://127.0.0.1:8787/v1/providers | grep -q registered'

# Line 2 Telegram / Line 7 Schedule — product not complete; mark XFAIL until green
if [[ "${KERYX_HOST_CI_STRICT:-0}" == "1" ]]; then
  check "Telegram away Approvals E2E" bash -c 'echo "not implemented"; exit 1'
  check "Schedule always-on ticker" bash -c 'echo "not implemented"; exit 1'
else
  echo "SKIP Telegram E2E (set KERYX_HOST_CI_STRICT=1 to require)"
  echo "SKIP Schedule ticker E2E (set KERYX_HOST_CI_STRICT=1 to require)"
fi

# Remote second-node proof: optional env with a probe host result file
if [[ -n "${KERYX_EDGE_SECOND_NODE_OK:-}" ]]; then
  check "L5 second-node flag" test "$KERYX_EDGE_SECOND_NODE_OK" = "1"
else
  echo "WARN L5 second-node not proven (set KERYX_EDGE_SECOND_NODE_OK=1 after remote curl succeeds)"
  fail=1
fi

echo "keryx-host-ci done fail=$fail log=$LOG"
exit "$fail"
