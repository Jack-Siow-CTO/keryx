#!/usr/bin/env bash
# End-to-end control-plane smoke against a running Worker.
# Requires: curl, and optionally python3 or jq for JSON.
set -euo pipefail

KERYX_URL="${KERYX_URL:-http://127.0.0.1:8787}"
TOKEN="${KERYX_OPERATOR_TOKEN:-}"
PROVIDER="${KERYX_SMOKE_PROVIDER:-}"
TIMEOUT_SECS="${KERYX_SMOKE_TIMEOUT_SECS:-60}"

die() {
  echo "smoke: ERROR: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

json_field() {
  # json_field <json> <field>
  local json="$1" field="$2"
  if command -v jq >/dev/null 2>&1; then
    jq -r --arg f "$field" '.[$f] // empty' <<<"${json}"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get(sys.argv[1],"") or "")' "${field}" <<<"${json}"
  else
    die "need jq or python3 to parse JSON"
  fi
}

need_cmd curl
[[ -n "${TOKEN}" ]] || die "set KERYX_OPERATOR_TOKEN (same token the Worker uses)"

echo "smoke: health @ ${KERYX_URL}"
health="$(curl -fsS --max-time 5 "${KERYX_URL}/health")" || die "health request failed — is keryx running?"
echo "  ${health}"
echo "${health}" | grep -q '"status"' || die "unexpected health body"

echo "smoke: unauthenticated POST /v1/sessions must be 401"
code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 -X POST "${KERYX_URL}/v1/sessions" || true)"
[[ "${code}" == "401" ]] || die "expected 401 without token, got ${code}"

echo "smoke: create session"
session_json="$(curl -fsS --max-time 10 -X POST "${KERYX_URL}/v1/sessions" \
  -H "authorization: Bearer ${TOKEN}")" || die "create session failed"
session_id="$(json_field "${session_json}" id)"
[[ -n "${session_id}" ]] || die "no session id in: ${session_json}"
echo "  session_id=${session_id}"

goal='{"goal":"keryx smoke test"}'
if [[ -n "${PROVIDER}" ]]; then
  if command -v jq >/dev/null 2>&1; then
    goal="$(jq -nc --arg g "keryx smoke test" --arg p "${PROVIDER}" '{goal:$g, provider:$p}')"
  elif command -v python3 >/dev/null 2>&1; then
    goal="$(python3 -c 'import json,sys; print(json.dumps({"goal":"keryx smoke test","provider":sys.argv[1]}))' "${PROVIDER}")"
  else
    die "need jq or python3 when KERYX_SMOKE_PROVIDER is set"
  fi
fi

echo "smoke: start run"
run_json="$(curl -fsS --max-time 15 -X POST "${KERYX_URL}/v1/sessions/${session_id}/runs" \
  -H "authorization: Bearer ${TOKEN}" \
  -H "content-type: application/json" \
  -d "${goal}")" || die "start run failed"
run_id="$(json_field "${run_json}" id)"
[[ -n "${run_id}" ]] || die "no run id in: ${run_json}"
echo "  run_id=${run_id}"

echo "smoke: wait for terminal run status (timeout ${TIMEOUT_SECS}s)"
deadline=$((SECONDS + TIMEOUT_SECS))
status=""
while (( SECONDS < deadline )); do
  body="$(curl -fsS --max-time 10 "${KERYX_URL}/v1/runs/${run_id}" \
    -H "authorization: Bearer ${TOKEN}")" || die "get run failed"
  status="$(json_field "${body}" status)"
  case "${status}" in
    completed|failed|cancelled|interrupted)
      echo "  status=${status}"
      result="$(json_field "${body}" result)"
      if [[ -n "${result}" ]]; then
        echo "  result=${result}"
      fi
      if [[ "${status}" == "completed" ]]; then
        echo "smoke: OK"
        exit 0
      fi
      die "run ended with status=${status}"
      ;;
    *)
      sleep 0.5
      ;;
  esac
done

die "timed out waiting for run ${run_id} (last status=${status:-unknown})"
