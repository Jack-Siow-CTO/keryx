#!/usr/bin/env bash
# Generate a strong operator token into a mode-600 file (default: stdout path).
set -euo pipefail

OUT="${1:-}"
if [[ -z "${OUT}" ]]; then
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'
    echo
  fi
  exit 0
fi

mkdir -p "$(dirname "${OUT}")"
if command -v openssl >/dev/null 2>&1; then
  openssl rand -hex 32 >"${OUT}"
else
  head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' >"${OUT}"
  echo >>"${OUT}"
fi
chmod 600 "${OUT}"
echo "Wrote operator token to ${OUT} (mode 600)" >&2
