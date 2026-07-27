# Install Keryx

Public entry for operators who want a Worker on their machine. Security model stays fail-closed: **loopback-only bind**, **operator bearer token**, Tailscale for reachability only.

**Prereq for remote clients:** complete a local install here, then optionally [tailnet-edge.md](./tailnet-edge.md).

## Paths

| Mode | Config | Data | Binary |
|------|--------|------|--------|
| **User** (default) | `~/.config/keryx/env` | `~/.local/share/keryx` | `~/.local/bin/keryx` (via `cargo install`) |
| **System** (Linux) | `/etc/keryx/env` | `/var/lib/keryx` | `/usr/local/bin/keryx` |

## Method 1 — From source (recommended)

Requires [Rust](https://rustup.rs/) (stable).

```bash
git clone https://github.com/Jack-Siow-CTO/keryx.git
cd keryx
./scripts/install.sh
```

What the script does:

1. Builds and installs the `keryx` binary with Cargo
2. Creates config + data directories
3. Writes an env file from `.env.example` if missing
4. Generates an operator token if still set to the placeholder
5. Prints next-step commands

Flags:

| Flag | Meaning |
|------|---------|
| `--user` | User install (default) |
| `--system` | System install under `/etc/keryx` + `/var/lib/keryx` (needs root) |
| `--no-service` | Do not install systemd unit |
| `--from-source` | Force Cargo build (default when no release binary) |

Manual equivalent:

```bash
cargo install --path crates/worker --locked
mkdir -p ~/.config/keryx ~/.local/share/keryx
cp .env.example ~/.config/keryx/env
chmod 600 ~/.config/keryx/env
./scripts/gen-operator-token.sh ~/.config/keryx/operator.token
# put token into env file as KERYX_OPERATOR_TOKEN or KERYX_OPERATOR_TOKEN_FILE
```

## Method 2 — Foreground without install script

```bash
export KERYX_OPERATOR_TOKEN="$(openssl rand -hex 32)"
export KERYX_DATA_DIR=./data
export KERYX_BIND=127.0.0.1:8787
export KERYX_DEFAULT_PROVIDER=fake
cargo run -p keryx-worker --release
```

## Method 3 — Docker (optional)

Prefer the native binary on always-on hosts. Docker is for operators who already standardize on containers.

```bash
cp .env.example .env
# edit .env — set a real KERYX_OPERATOR_TOKEN
docker compose up --build -d
curl -sS http://127.0.0.1:8787/health
```

Compose publishes **only** `127.0.0.1:8787` on the host.

## Configure

Edit the env file (user: `~/.config/keryx/env`). Full template: [../../.env.example](../../.env.example).

| Variable | Required | Notes |
|----------|----------|-------|
| `KERYX_OPERATOR_TOKEN` or `*_FILE` | yes | Bearer for `/v1/*` |
| `KERYX_BIND` | no | Default `127.0.0.1:8787`; must be loopback |
| `KERYX_DATA_DIR` | no | SQLite directory |
| `KERYX_DEFAULT_PROVIDER` | no | `fake` until keys exist |
| `OPENAI_API_KEY` / `XAI_API_KEY` | for real models | Official APIs |
| `KERYX_WORKSPACE_ROOTS` | for file tools | Colon-separated paths |

Load env into a shell before running the binary:

```bash
set -a
source ~/.config/keryx/env
set +a
keryx doctor   # optional readiness checks
keryx          # start Worker
```

Systemd loads `/etc/keryx/env` via `EnvironmentFile` (no manual source).

## Run

### Foreground

```bash
set -a && source ~/.config/keryx/env && set +a
keryx
```

### systemd (Linux)

After `./scripts/install.sh --system` (or copy `deploy/keryx.service` yourself):

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now keryx
sudo systemctl status keryx
curl -sS http://127.0.0.1:8787/health
```

User unit (no root data dirs): install unit under `~/.config/systemd/user/` and use `systemctl --user enable --now keryx`.

### Verify

```bash
export KERYX_URL=http://127.0.0.1:8787
export KERYX_OPERATOR_TOKEN=...   # same as Worker
./scripts/smoke.sh
```

## Real models

1. Set `OPENAI_API_KEY` and/or `XAI_API_KEY` in the env file.
2. Set `KERYX_DEFAULT_PROVIDER=openai` or `grok`.
3. Restart the Worker.
4. Start a Run with `"provider":"openai"` (or rely on default).

Live adapter tests (opt-in): [live-model-verification.md](./live-model-verification.md).

## Upgrade

```bash
cd /path/to/keryx
git pull
./scripts/install.sh --from-source
# system:
sudo systemctl restart keryx
```

## Uninstall (user)

```bash
rm -f ~/.local/bin/keryx
# optional: rm -rf ~/.config/keryx ~/.local/share/keryx
```

## Security checklist

- [ ] Operator token is long and private (mode `600` files)
- [ ] Worker binds loopback only
- [ ] API keys never committed to git
- [ ] Remote access only via Tailnet HTTPS edge — not Funnel/public bind
- [ ] Prefer official API keys over consumer web sessions

## Related

- [operator-checklist.md](./operator-checklist.md) — levels to declare “ready to use”
- [tailnet-edge.md](./tailnet-edge.md) — Mac/phone over Tailscale
- [live-model-verification.md](./live-model-verification.md)
- [consumer-web-sessions.md](./consumer-web-sessions.md) — advanced, unofficial
