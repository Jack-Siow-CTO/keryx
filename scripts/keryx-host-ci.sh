#!/usr/bin/env bash
# Required host job for daily-use bar (ADR 0035 / issue #80).
# Exit non-zero until all host scenarios pass. Safe under systemd timer or manual run.
# Do not declare green with SKIP. Never print secrets (tokens, bot tokens).
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
CONFIG="${KERYX_CONFIG_DIR:-$HOME/.config/keryx}"
# shellcheck disable=SC1091
set -a; source "$CONFIG/env"; set +a

EDGE_URL="${KERYX_EDGE_URL:-https://jack-agent-worker.tail68a74b.ts.net:8443}"
EDGE_HOST="${KERYX_EDGE_HOST:-jack-agent-worker.tail68a74b.ts.net}"
EDGE_IP="${KERYX_EDGE_IP:-100.108.132.109}"
EDGE_PORT="${KERYX_EDGE_PORT:-8443}"
LOOPBACK="${KERYX_URL:-http://127.0.0.1:8787}"
LOG_DIR="${KERYX_HOST_CI_LOG:-$HOME/.local/share/keryx/host-ci}"
mkdir -p "$LOG_DIR"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
LOG="$LOG_DIR/run-$STAMP.log"
exec > >(tee -a "$LOG") 2>&1

echo "keryx-host-ci start $STAMP"
fail=0

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing command: $1"
    return 1
  }
}

json_get() {
  # json_get <json> <python-expr-on-d>  e.g. d.get("id")
  local json="$1" expr="$2"
  python3 -c "import json,sys; d=json.load(sys.stdin); v=($expr); print(v if v is not None else '')" <<<"$json"
}

# journalctl | grep -q under pipefail can fail with SIGPIPE (141) even on match.
journal_has() {
  local pattern="$1"
  local out
  out="$(journalctl --user -u keryx -b --no-pager 2>/dev/null || true)"
  grep -F -q "$pattern" <<<"$out"
}

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

auth_hdr() {
  printf 'authorization: Bearer %s' "$KERYX_OPERATOR_TOKEN"
}

# --- Line 8: L4 always-on Worker ---
check "L4 user unit active" systemctl --user is-active --quiet keryx
check "L4 loopback health" bash -c "curl -fsS --max-time 5 '$LOOPBACK/health' | grep -q ok"
check "L4 doctor" bash -c 'keryx doctor >/dev/null'

# --- Line 8: L5 Edge on-host ---
check "L5 on-host Edge health" bash -c \
  "curl -fsS --max-time 10 --resolve ${EDGE_HOST}:${EDGE_PORT}:${EDGE_IP} '${EDGE_URL}/health' | grep -q ok"
check "L5 Edge auth challenge" bash -c \
  "code=\$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' --resolve ${EDGE_HOST}:${EDGE_PORT}:${EDGE_IP} -X POST '${EDGE_URL}/v1/sessions'); test \"\$code\" = 401"

# --- Line 8: provider diagnostic ---
check "provider providers endpoint" bash -c \
  "curl -fsS --max-time 10 -H \"\$(printf 'authorization: Bearer %s' \"\$KERYX_OPERATOR_TOKEN\")\" '$LOOPBACK/v1/providers' | grep -q registered"

# Provider diagnostic Run (one short control-plane Run when model is available).
scenario_provider_diagnostic_run() {
  need_cmd curl
  need_cmd python3
  [[ -n "${KERYX_OPERATOR_TOKEN:-}" ]] || {
    echo "KERYX_OPERATOR_TOKEN missing"
    return 1
  }
  local session run_id body status deadline
  session="$(curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/sessions" \
    -H "$(auth_hdr)")" || return 1
  local sid
  sid="$(json_get "$session" 'd.get("id")')"
  [[ -n "$sid" ]] || {
    echo "no session id"
    return 1
  }
  run_id="$(curl -fsS --max-time 15 -X POST "$LOOPBACK/v1/sessions/${sid}/runs" \
    -H "$(auth_hdr)" -H 'content-type: application/json' \
    -d '{"goal":"host-ci provider diagnostic: reply with exactly the word pong and nothing else"}' \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')"
  [[ -n "$run_id" ]] || {
    echo "no run id"
    return 1
  }
  echo "  diagnostic run_id=$run_id"
  deadline=$((SECONDS + ${KERYX_HOST_CI_DIAG_TIMEOUT_SECS:-90}))
  status=""
  while ((SECONDS < deadline)); do
    body="$(curl -fsS --max-time 10 "$LOOPBACK/v1/runs/${run_id}" -H "$(auth_hdr)")" || return 1
    status="$(json_get "$body" 'd.get("status")')"
    case "$status" in
      completed)
        echo "  diagnostic status=completed"
        return 0
        ;;
      failed|cancelled|interrupted)
        echo "  diagnostic status=$status (auth path exercised; accept non-completed as red only if never started)"
        # Fail closed: diagnostic must complete for line 8 "provider auth valid enough for one diagnostic Run".
        return 1
        ;;
    esac
    sleep 1
  done
  echo "  diagnostic timed out status=${status:-unknown}"
  curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/runs/${run_id}/cancel" -H "$(auth_hdr)" >/dev/null 2>&1 || true
  return 1
}
check "provider diagnostic Run" scenario_provider_diagnostic_run

# --- Line 8: second-node Edge proof ---
# Host job runs on jack-agent-worker; second-node proof is recorded after a remote curl
# (proof file and/or KERYX_EDGE_SECOND_NODE_OK=1). Re-check recorded evidence when present.
scenario_second_node() {
  local proof="$LOG_DIR/second-node-proof.txt"
  if [[ "${KERYX_EDGE_SECOND_NODE_OK:-}" == "1" ]]; then
    if [[ -f "$proof" ]]; then
      grep -q 'health HTTP 200' "$proof" || {
        echo "proof file missing health HTTP 200"
        return 1
      }
      grep -q 'sessions unauth HTTP 401' "$proof" || {
        echo "proof file missing sessions unauth HTTP 401"
        return 1
      }
      echo "  second-node proof file ok"
    else
      echo "  second-node flag set (no proof file; accept flag only)"
    fi
    return 0
  fi
  echo "set KERYX_EDGE_SECOND_NODE_OK=1 after remote curl from a second tailnet node succeeds"
  return 1
}
check "L5 second-node Edge proof" scenario_second_node

# --- Line 2: Telegram Away Approvals (host wiring + live notify + control-plane decide) ---
# Host bar proves allowlisted Gateway is live, Bot API can notify the allowlisted chat with
# Approve/Deny markup, and control-plane Approve/Deny is system of record.
# Pending Approvals are seeded against a real Run row (openai_codex on this host does not
# emit tool_calls, so model-driven high-blast is not a reliable host trigger).
scenario_telegram_away_approvals() {
  need_cmd curl
  need_cmd python3
  [[ -n "${KERYX_TELEGRAM_BOT_TOKEN:-}" ]] || {
    echo "KERYX_TELEGRAM_BOT_TOKEN missing"
    return 1
  }
  [[ -n "${KERYX_TELEGRAM_ALLOWED_CHAT_IDS:-}" ]] || {
    echo "KERYX_TELEGRAM_ALLOWED_CHAT_IDS missing (open allowlist not accepted for host bar)"
    return 1
  }
  # getMe — token valid (never print token)
  local me
  me="$(curl -fsS --max-time 15 "https://api.telegram.org/bot${KERYX_TELEGRAM_BOT_TOKEN}/getMe")" || {
    echo "Telegram getMe failed"
    return 1
  }
  python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok") is True; print("  bot ok username=", d["result"].get("username","?"))' <<<"$me" || return 1

  journal_has 'telegram gateway long-poll starting' || {
    echo "journal missing telegram gateway long-poll starting this boot"
    return 1
  }
  echo "  telegram gateway long-poll present this boot"

  local chat_id
  chat_id="$(python3 -c 'import os; print(os.environ["KERYX_TELEGRAM_ALLOWED_CHAT_IDS"].split(",")[0].strip())')"
  [[ -n "$chat_id" ]] || {
    echo "empty allowlist chat id"
    return 1
  }
  echo "  allowlisted chat configured (closed allowlist)"

  # Approvals unauthenticated must fail closed
  local code
  code="$(curl -sS --max-time 5 -o /dev/null -w '%{http_code}' "$LOOPBACK/v1/approvals" || true)"
  [[ "$code" == "401" ]] || {
    echo "expected 401 without token on /v1/approvals got $code"
    return 1
  }
  echo "  approvals unauth 401"

  # Anchor Run for FK (complete or active is fine)
  local sid run_id
  sid="$(curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/sessions" -H "$(auth_hdr)" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')"
  [[ -n "$sid" ]] || return 1
  run_id="$(curl -fsS --max-time 15 -X POST "$LOOPBACK/v1/sessions/${sid}/runs" \
    -H "$(auth_hdr)" -H 'content-type: application/json' \
    -d '{"goal":"host-ci away-approvals anchor: reply with the word anchor only"}' \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')"
  [[ -n "$run_id" ]] || {
    echo "anchor run failed"
    return 1
  }
  echo "  anchor run_id=$run_id"

  # Seed two pending Approvals into durable store (same shape as agent-loop high-blast rows).
  local db_path approval_a approval_d principal
  db_path="${KERYX_DATA_DIR:-$HOME/.local/share/keryx}/keryx.db"
  principal="${KERYX_OPERATOR_PRINCIPAL:-operator}"
  [[ -f "$db_path" ]] || {
    echo "missing db $db_path"
    return 1
  }
  eval "$(python3 -c '
import sqlite3, sys, uuid
db, run_id, principal = sys.argv[1], sys.argv[2], sys.argv[3]
a = str(uuid.uuid4())
d = str(uuid.uuid4())
conn = sqlite3.connect(db, timeout=30)
conn.execute("PRAGMA busy_timeout=30000")
conn.execute(
    "INSERT INTO approvals (id, run_id, action, summary, status, requested_by, decided_by) VALUES (?,?,?,?,?,?,?)",
    (a, run_id, "run_terminal", "host-ci approve probe cmd=echo host-ci-away", "pending", principal, None),
)
conn.execute(
    "INSERT INTO approvals (id, run_id, action, summary, status, requested_by, decided_by) VALUES (?,?,?,?,?,?,?)",
    (d, run_id, "run_terminal", "host-ci deny probe cmd=echo host-ci-deny", "pending", principal, None),
)
conn.commit()
conn.close()
print(f"approval_a={a}")
print(f"approval_d={d}")
' "$db_path" "$run_id" "$principal")"
  [[ -n "${approval_a:-}" && -n "${approval_d:-}" ]] || {
    echo "failed to seed pending Approvals"
    return 1
  }
  echo "  seeded pending approvals approve=$approval_a deny=$approval_d"

  # Control plane must list them
  local body listed
  body="$(curl -fsS --max-time 10 "$LOOPBACK/v1/approvals?pending=true" -H "$(auth_hdr)")" || return 1
  listed="$(python3 -c '
import json,sys
d=json.load(sys.stdin)
ids={a.get("id") for a in (d.get("approvals") or [])}
print("ok" if sys.argv[1] in ids and sys.argv[2] in ids else "missing")
' "$approval_a" "$approval_d" <<<"$body")"
  [[ "$listed" == "ok" ]] || {
    echo "seeded Approvals not visible via control plane list"
    return 1
  }
  echo "  control plane lists pending Approvals"

  # Live notify proves Bot API markup shape for Away Approvals (same wire as Gateway).
  # Label host-ci and strip the keyboard after control-plane decide so operators are not
  # left with dead Approve buttons that return "approval is not pending" on tap.
  local markup notify_body notify_resp message_id
  markup="$(python3 -c "
import json,sys
aid=sys.argv[1]
print(json.dumps({
  'inline_keyboard': [[
    {'text':'Approve','callback_data': f'a:{aid}'},
    {'text':'Deny','callback_data': f'd:{aid}'},
  ]]
}))
" "$approval_a")"
  notify_body="$(python3 -c "
import json,sys
print(json.dumps({
  'chat_id': sys.argv[1],
  'text': '[host-ci] Away Approvals probe (auto-resolves — do not tap)\\naction: run_terminal\\nsummary: host-ci approve probe\\nid: '+sys.argv[2]+'\\nrun: '+sys.argv[3],
  'reply_markup': json.loads(sys.argv[4]),
}))
" "$chat_id" "$approval_a" "$run_id" "$markup")"
  notify_resp="$(curl -fsS --max-time 20 \
    -X POST "https://api.telegram.org/bot${KERYX_TELEGRAM_BOT_TOKEN}/sendMessage" \
    -H 'content-type: application/json' \
    -d "$notify_body")" || {
    echo "Telegram sendMessage notify failed"
    return 1
  }
  message_id="$(python3 -c '
import json,sys
d=json.load(sys.stdin)
assert d.get("ok") is True, d
mid=(d.get("result") or {}).get("message_id")
assert mid is not None, d
print(mid)
print("  live notify to allowlisted chat ok", file=sys.stderr)
' <<<"$notify_resp")" || return 1
  echo "  live notify message_id=$message_id"

  # Approve (SoR) — continues without Policy escalate (decide only resolves the row).
  local decided
  decided="$(curl -fsS --max-time 10 -X POST \
    "$LOOPBACK/v1/approvals/${approval_a}/approve" -H "$(auth_hdr)")" || return 1
  python3 -c '
import json,sys
d=json.load(sys.stdin)
assert d.get("status")=="approved", d
assert d.get("decided_by"), "missing decided_by"
print("  approve status=approved decided_by=", d.get("decided_by"))
' <<<"$decided" || return 1

  # Deny fail-closed
  decided="$(curl -fsS --max-time 10 -X POST \
    "$LOOPBACK/v1/approvals/${approval_d}/deny" -H "$(auth_hdr)")" || return 1
  python3 -c '
import json,sys
d=json.load(sys.stdin)
assert d.get("status")=="denied", d
print("  deny status=denied (fail closed)")
' <<<"$decided" || return 1

  # Remove inline keyboard + rewrite body so host-ci leave no clickable dead buttons.
  local edit_body edit_resp
  edit_body="$(python3 -c "
import json,sys
print(json.dumps({
  'chat_id': sys.argv[1],
  'message_id': int(sys.argv[2]),
  'text': '[host-ci] Away Approvals probe — auto-resolved (approved+denied via control plane). Keyboard removed.',
  'reply_markup': {'inline_keyboard': []},
}))
" "$chat_id" "$message_id")"
  edit_resp="$(curl -fsS --max-time 20 \
    -X POST "https://api.telegram.org/bot${KERYX_TELEGRAM_BOT_TOKEN}/editMessageText" \
    -H 'content-type: application/json' \
    -d "$edit_body")" || {
    echo "Telegram editMessageText (strip keyboard) failed"
    return 1
  }
  python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok") is True, d; print("  host-ci notify keyboard stripped")' <<<"$edit_resp" || return 1

  # Non-allowlisted: closed allowlist is enforced by Gateway (empty ALLOWED_CHAT_IDS rejected above).
  # Double-check env is not open.
  python3 -c '
import os
raw=os.environ.get("KERYX_TELEGRAM_ALLOWED_CHAT_IDS","").strip()
assert raw, "allowlist empty"
print("  allowlist closed (non-empty fixed targets)")
' || return 1

  # Cancel anchor run if still active
  curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/runs/${run_id}/cancel" -H "$(auth_hdr)" >/dev/null 2>&1 || true

  echo "  Telegram Away Approvals host path green"
  return 0
}
check "Telegram Away Approvals" scenario_telegram_away_approvals

# --- Line 7 host half: Schedule fire under always-on systemd ticker ---
# Do NOT use POST /v1/schedules/tick — that is the GHA seam. Host proves in-process ticker.
scenario_schedule_always_on() {
  need_cmd curl
  need_cmd python3
  [[ -n "${KERYX_OPERATOR_TOKEN:-}" ]] || return 1

  # Ticker must be running under the systemd Worker (log line from current boot).
  journal_has 'schedule ticker starting (always-on)' || {
    echo "journal missing schedule ticker starting (always-on) — deploy Worker with #78 ticker"
    return 1
  }
  echo "  schedule ticker present this boot"

  local now next_fire create sid_sched interval goal
  now="$(date +%s)"
  # Due immediately; large interval so only one fire during the wait window.
  next_fire="$now"
  interval=3600
  goal="host-ci schedule fire: reply with the single word scheduled and stop"
  create="$(curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/schedules" \
    -H "$(auth_hdr)" -H 'content-type: application/json' \
    -d "$(python3 -c "
import json
print(json.dumps({
  'goal': '''$goal''',
  'interval_secs': $interval,
  'next_fire_at': $next_fire,
  'policy_tools': ['read_file','memory_read','skills_list','skill_load'],
}))
")")" || {
    echo "create schedule failed"
    return 1
  }
  local schedule_id
  schedule_id="$(json_get "$create" 'd.get("id")')"
  [[ -n "$schedule_id" ]] || {
    echo "no schedule id: $create"
    return 1
  }
  local tools
  tools="$(json_get "$create" ' ",".join(d.get("policy_tools") or []) ')"
  echo "  schedule_id=$schedule_id frozen_tools=$tools"
  python3 -c '
import json,sys
d=json.load(sys.stdin)
tools=d.get("policy_tools") or []
assert "read_file" in tools, tools
assert "write_file" not in tools, "frozen tools must stay reduced (no write_file)"
' <<<"$create" || {
    echo "frozen policy_tools not reduced"
    curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/schedules/${schedule_id}/delete" -H "$(auth_hdr)" >/dev/null 2>&1 || true
    return 1
  }

  # Wait for always-on ticker (default 30s; allow miss + one period).
  local wait_secs body last_fired session_id run_id origin
  wait_secs="${KERYX_HOST_CI_SCHEDULE_WAIT_SECS:-120}"
  local deadline=$((SECONDS + wait_secs))
  last_fired=""
  session_id=""
  while ((SECONDS < deadline)); do
    body="$(curl -fsS --max-time 10 "$LOOPBACK/v1/schedules" -H "$(auth_hdr)")" || return 1
    last_fired="$(SCHED_ID="$schedule_id" python3 -c '
import json,sys,os
d=json.load(sys.stdin)
sid=os.environ["SCHED_ID"]
for s in d.get("schedules") or []:
    if s.get("id")==sid:
        lf=s.get("last_fired_at")
        print("" if lf is None else lf)
        break
' <<<"$body")"
    session_id="$(SCHED_ID="$schedule_id" python3 -c '
import json,sys,os
d=json.load(sys.stdin)
sid=os.environ["SCHED_ID"]
for s in d.get("schedules") or []:
    if s.get("id")==sid:
        print(s.get("session_id") or "")
        break
' <<<"$body")"
    if [[ -n "$last_fired" && -n "$session_id" ]]; then
      break
    fi
    sleep 2
  done

  if [[ -z "$last_fired" || -z "$session_id" ]]; then
    echo "schedule did not fire under always-on ticker within ${wait_secs}s"
    curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/schedules/${schedule_id}/delete" -H "$(auth_hdr)" >/dev/null 2>&1 || true
    return 1
  fi
  echo "  last_fired_at=$last_fired session_id=$session_id"

  # Prove Run origin=schedule via Session projection or GET run.
  origin=""
  run_id=""
  local i sess
  for i in 1 2 3 4 5 6 7 8 9 10; do
    sess="$(curl -fsS --max-time 10 "$LOOPBACK/v1/sessions/${session_id}" -H "$(auth_hdr)")" || return 1
    run_id="$(python3 -c '
import json,sys
d=json.load(sys.stdin)
ar=d.get("active_root_run") or {}
print(ar.get("id") or "")
' <<<"$sess")"
    origin="$(python3 -c '
import json,sys
d=json.load(sys.stdin)
ar=d.get("active_root_run") or {}
print(ar.get("origin") or "")
' <<<"$sess")"
    if [[ -n "$run_id" ]]; then
      origin="$(curl -fsS --max-time 10 "$LOOPBACK/v1/runs/${run_id}" -H "$(auth_hdr)" \
        | python3 -c 'import json,sys; print(json.load(sys.stdin).get("origin",""))')"
      break
    fi
    sleep 1
  done

  if [[ "$origin" != "schedule" ]]; then
    echo "expected origin=schedule got origin=${origin:-empty} run_id=${run_id:-none}"
    curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/schedules/${schedule_id}/delete" -H "$(auth_hdr)" >/dev/null 2>&1 || true
    return 1
  fi
  echo "  fired run origin=schedule run_id=$run_id"

  curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/schedules/${schedule_id}/delete" -H "$(auth_hdr)" >/dev/null || true
  if [[ -n "${run_id:-}" ]]; then
    curl -fsS --max-time 10 -X POST "$LOOPBACK/v1/runs/${run_id}/cancel" -H "$(auth_hdr)" >/dev/null 2>&1 || true
  fi
  echo "  Schedule always-on ticker host path green"
  return 0
}
check "Schedule always-on ticker" scenario_schedule_always_on

echo "keryx-host-ci done fail=$fail log=$LOG"
exit "$fail"
