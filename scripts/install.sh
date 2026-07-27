#!/usr/bin/env bash
# Install Keryx Worker binary + config dirs (from source).
# Usage:
#   ./scripts/install.sh              # user install
#   sudo ./scripts/install.sh --system
#   ./scripts/install.sh --no-service
set -euo pipefail

MODE="user"
INSTALL_SERVICE=1
FORCE_SOURCE=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user) MODE="user"; shift ;;
    --system) MODE="system"; shift ;;
    --no-service) INSTALL_SERVICE=0; shift ;;
    --from-source) FORCE_SOURCE=1; shift ;;
    --binary)
      echo "install: prebuilt binary mode is not available yet; use --from-source" >&2
      exit 1
      ;;
    -h|--help)
      sed -n '2,8p' "$0"
      exit 0
      ;;
    *)
      echo "install: unknown flag: $1" >&2
      exit 1
      ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

die() { echo "install: ERROR: $*" >&2; exit 1; }
info() { echo "install: $*"; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

if [[ "${MODE}" == "system" && "$(id -u)" -ne 0 ]]; then
  die "--system requires root (try: sudo ./scripts/install.sh --system)"
fi

need_cmd cargo
need_cmd install

if [[ "${MODE}" == "user" ]]; then
  CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/keryx"
  DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/keryx"
  BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
  ENV_FILE="${CONFIG_DIR}/env"
  SYSTEMD_UNIT=""
  TOKEN_FILE="${CONFIG_DIR}/operator.token"
else
  CONFIG_DIR="/etc/keryx"
  DATA_DIR="/var/lib/keryx"
  BIN_DIR="/usr/local/bin"
  ENV_FILE="${CONFIG_DIR}/env"
  SYSTEMD_UNIT="/etc/systemd/system/keryx.service"
  TOKEN_FILE="${CONFIG_DIR}/operator.token"
fi

info "mode=${MODE}"
info "building keryx from source (release)…"
cargo build -p keryx-worker --release

BIN_SRC="${ROOT}/target/release/keryx"
[[ -x "${BIN_SRC}" ]] || die "build succeeded but binary missing at ${BIN_SRC}"

mkdir -p "${CONFIG_DIR}" "${DATA_DIR}" "${BIN_DIR}"
install -m 755 "${BIN_SRC}" "${BIN_DIR}/keryx"
info "installed binary → ${BIN_DIR}/keryx"

if [[ ! -f "${ENV_FILE}" ]]; then
  if [[ -f "${ROOT}/.env.example" ]]; then
    cp "${ROOT}/.env.example" "${ENV_FILE}"
  else
    cat >"${ENV_FILE}" <<'EOF'
KERYX_OPERATOR_TOKEN=change-me-generate-a-long-random-token
KERYX_BIND=127.0.0.1:8787
KERYX_DATA_DIR=./data
KERYX_DEFAULT_PROVIDER=fake
EOF
  fi
  chmod 600 "${ENV_FILE}"
  info "wrote ${ENV_FILE} (mode 600) — edit secrets before production use"
else
  info "keeping existing ${ENV_FILE}"
fi

# Point data dir at the install data directory when still default-ish.
if grep -qE '^KERYX_DATA_DIR=\./data$' "${ENV_FILE}" 2>/dev/null; then
  # Use a portable in-place edit
  tmp="$(mktemp)"
  sed "s|^KERYX_DATA_DIR=\\./data\$|KERYX_DATA_DIR=${DATA_DIR}|" "${ENV_FILE}" >"${tmp}"
  mv "${tmp}" "${ENV_FILE}"
  chmod 600 "${ENV_FILE}"
  info "set KERYX_DATA_DIR=${DATA_DIR}"
fi

if grep -qE '^KERYX_OPERATOR_TOKEN=change-me' "${ENV_FILE}" 2>/dev/null; then
  need_cmd bash
  TOKEN="$("${ROOT}/scripts/gen-operator-token.sh")"
  tmp="$(mktemp)"
  sed "s|^KERYX_OPERATOR_TOKEN=change-me.*|KERYX_OPERATOR_TOKEN=${TOKEN}|" "${ENV_FILE}" >"${tmp}"
  mv "${tmp}" "${ENV_FILE}"
  chmod 600 "${ENV_FILE}"
  umask 077
  printf '%s\n' "${TOKEN}" >"${TOKEN_FILE}"
  chmod 600 "${TOKEN_FILE}"
  info "generated operator token (also saved to ${TOKEN_FILE})"
fi

if [[ "${MODE}" == "system" ]]; then
  if ! id keryx >/dev/null 2>&1; then
    if command -v useradd >/dev/null 2>&1; then
      useradd --system --home "${DATA_DIR}" --shell /usr/sbin/nologin keryx || true
    elif command -v adduser >/dev/null 2>&1; then
      adduser --system --home "${DATA_DIR}" --shell /usr/sbin/nologin keryx || true
    else
      info "warning: could not create system user 'keryx'; edit the unit User= line"
    fi
  fi
  chown -R keryx:keryx "${DATA_DIR}" 2>/dev/null || true
  chown root:keryx "${ENV_FILE}" 2>/dev/null || true
  chmod 640 "${ENV_FILE}" 2>/dev/null || chmod 600 "${ENV_FILE}"
fi

if [[ "${INSTALL_SERVICE}" -eq 1 && "${MODE}" == "system" && -d /etc/systemd/system ]]; then
  install -m 644 "${ROOT}/deploy/keryx.service" "${SYSTEMD_UNIT}"
  # Ensure ExecStart matches installed binary
  if [[ "${BIN_DIR}/keryx" != "/usr/local/bin/keryx" ]]; then
    tmp="$(mktemp)"
    sed "s|^ExecStart=.*|ExecStart=${BIN_DIR}/keryx|" "${SYSTEMD_UNIT}" >"${tmp}"
    mv "${tmp}" "${SYSTEMD_UNIT}"
  fi
  info "installed ${SYSTEMD_UNIT}"
  if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
  fi
elif [[ "${INSTALL_SERVICE}" -eq 1 && "${MODE}" == "user" ]]; then
  UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  if [[ -d "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" ]] || command -v systemctl >/dev/null 2>&1; then
    mkdir -p "${UNIT_DIR}"
    cat >"${UNIT_DIR}/keryx.service" <<EOF
[Unit]
Description=Keryx agent Worker (user)
After=network-online.target

[Service]
Type=simple
EnvironmentFile=-${ENV_FILE}
WorkingDirectory=${DATA_DIR}
ExecStart=${BIN_DIR}/keryx
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
EOF
    info "wrote user unit ${UNIT_DIR}/keryx.service"
    info "enable with: systemctl --user daemon-reload && systemctl --user enable --now keryx"
  fi
fi

echo
info "done."
echo
echo "Next steps:"
echo "  1) Review config:  ${ENV_FILE}"
echo "  2) Optional real models: set OPENAI_API_KEY / XAI_API_KEY and KERYX_DEFAULT_PROVIDER"
if [[ "${MODE}" == "user" ]]; then
  echo "  3) Start:          set -a && source ${ENV_FILE} && set +a && ${BIN_DIR}/keryx"
  echo "     Doctor:         set -a && source ${ENV_FILE} && set +a && ${BIN_DIR}/keryx doctor"
  echo "  4) Smoke (other terminal, same env):"
  echo "       set -a && source ${ENV_FILE} && set +a"
  echo "       export KERYX_URL=http://127.0.0.1:8787"
  echo "       ${ROOT}/scripts/smoke.sh"
else
  echo "  3) Start service:  systemctl enable --now keryx"
  echo "  4) Health:         curl -sS http://127.0.0.1:8787/health"
  echo "  5) Smoke:          KERYX_OPERATOR_TOKEN=... ${ROOT}/scripts/smoke.sh"
fi
echo
echo "Docs: docs/deploy/install.md  |  docs/deploy/operator-checklist.md"
if [[ "${BIN_DIR}" == *".cargo/bin"* ]]; then
  case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *)
      echo
      echo "Note: add ${BIN_DIR} to your PATH if 'keryx' is not found."
      ;;
  esac
fi
