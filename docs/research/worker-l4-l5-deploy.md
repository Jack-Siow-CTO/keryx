# Research: jack-agent-worker L4/L5 deploy readiness

| Field | Value |
|-------|--------|
| Issue | [#67](https://github.com/Jack-Siow-CTO/keryx/issues/67) |
| Parent map | [#64](https://github.com/Jack-Siow-CTO/keryx/issues/64) |
| Date (UTC) | 2026-08-06 |
| Host probed | `jack-agent-worker` (Tailscale MagicDNS `jack-agent-worker.tail68a74b.ts.net`, IPv4 `100.108.132.109`) |
| Probe method | SSH read-only + local Tailnet curls from `jack-work-machine`. No install, unit, config, or binary changes. One diagnostic control-plane Session/Run on loopback (failed on expired Codex session; see §7). |
| Docs vs live | Host was **reachable**. Live facts below. Docs describe the intended ladder; this host uses a **user** install + separate Tailnet Caddy units, not `install.sh --system`. |

**Verdict (one line):** L4 is **substantially met** (always-on user systemd Worker + SQLite + loopback). L5 is **partially met** (Tailnet-bound Caddy Edge exists and works on-host) but **not daily-ready** from this probe: remote `:8443` from `jack-work-machine` timed out; only model path (`openai_codex`) has an **expired** OAuth token so real Runs fail.

---

## 1. Question

What is the factual readiness of **always-on Worker + Tailnet Edge** (operator checklist levels 4–5) for daily use on `jack-agent-worker`?

Sources: live host, `docs/deploy/operator-checklist.md`, `install.md`, `tailnet-edge.md`, `v2-agent-os.md`, `deploy/keryx.service`, `scripts/install.sh`.

---

## 2. Checklist map (docs promise → live)

| Level | Docs promise | Live on `jack-agent-worker` (2026-08-06) |
|-------|----------------|------------------------------------------|
| **L4** Always-on host | `sudo ./scripts/install.sh --system`; `systemctl enable --now keryx`; data under `/var/lib/keryx`; survives reboot; Sessions load after restart | **Not** system install. **User** unit `~/.config/systemd/user/keryx.service` is **enabled** and **active** since 2026-07-28. `loginctl Linger=yes` for `jacksiow`. Loopback health `{"status":"ok"}`. Data under `~/.local/share/keryx`. Reboot survival since last boot (2026-07-21) **not re-proven** in this probe (Worker start is after that boot). |
| **L5** Tailnet remote | Caddy (or equivalent) on Tailscale IPs only → `127.0.0.1:8787`; health over HTTPS from Mac/phone; auth Session+Run over Tailnet; no Funnel | Edge unit `jack-keryx-tailnet-proxy.service` **enabled/active**. Caddyfile binds Tailscale IPv4/IPv6 only, `reverse_proxy 127.0.0.1:8787`, site `https://jack-agent-worker.tail68a74b.ts.net:8443`. On-host Edge health + 401 without token **OK**. From `jack-work-machine`, TCP **443 open** (T3 app), TCP **8443 timeout**. Mac/phone **on tailnet** but **no** successful `:8443` client check from those devices in this probe. `tailscale serve` / `funnel`: none. |

---

## 3. Worker / systemd / install state

### 3.1 Process and unit

| Item | Fact |
|------|------|
| System unit `keryx.service` | **Absent** (`not-found` / inactive). No `/etc/keryx`, no `/var/lib/keryx`, no `/usr/local/bin/keryx`. |
| User unit | `~/.config/systemd/user/keryx.service` — **enabled**, **active (running)** since Tue 2026-07-28 07:29:09 UTC (~9+ days at probe). |
| Binary | `/home/jacksiow/.local/bin/keryx` — `keryx 0.1.0`, mtime 2026-07-28 07:29 UTC. |
| Main PID | Running as user `jacksiow`. |
| Linger | `Linger=yes` → user services can survive logout (good for always-on without system unit). |
| Last host boot | 2026-07-21 15:21 UTC (uptime ~15 days). Worker entered active **after** that boot (2026-07-28); full “survives reboot” check was **not** executed (would need reboot — out of scope). |

### 3.2 User unit shape (summary)

- `EnvironmentFile=-%h/.config/keryx/env`
- `WorkingDirectory=%h/.local/share/keryx`
- `ExecStartPre=%h/.config/keryx/sync-chatgpt-token.sh` (Codex token sync before start)
- `ExecStart=%h/.local/bin/keryx`
- `Restart=on-failure`

This matches the **user** path in `scripts/install.sh`, with an extra pre-start sync not in the stock unit template.

### 3.3 Host git tree (context only)

`/home/jacksiow/projects/keryx` at probe: `78d5a59` (2026-07-28) on `main`, messaging-shell era. Binary install date aligns with late July. Research laptop `main` at write time was newer (`c169263`); **host binary is older than current monorepo main** — upgrade gap for daily core features landed after 2026-07-28.

---

## 4. Data dir / SQLite posture

| Item | Fact |
|------|------|
| `KERYX_DATA_DIR` | `/home/jacksiow/.local/share/keryx` |
| DB file | `.../keryx.db` — SQLite 3.x, ~112 KiB, mode `644`, owner `jacksiow`, last write observed 2026-07-30 (and process still up). |
| Sidecars | `workspace/` (empty), `skills/` (empty) |
| ADR 0006 | Design: durable Sessions in local SQLite; interrupted Active Runs not mid-loop resumed. Live file present; Sessions list endpoint returned HTTP 200 with a non-empty list body (~4.6 KiB) — **durability in use**. |
| System paths | `/var/lib/keryx` **not** used (docs system-install default). |

**Posture:** User-local SQLite is the live store. Adequate for single-operator always-on if backups and disk are operator-owned. No separate backup job was observed. No `sqlite3` CLI on host for row counts.

---

## 5. Bind, health, auth (loopback)

| Check | Result |
|-------|--------|
| Listen | `127.0.0.1:8787` only (fail-closed loopback). |
| `GET /health` | `{"status":"ok"}` |
| `POST /v1/sessions` without token | **401** `missing authorization` |
| `GET /v1/providers` with operator token | **200** — default `openai_codex`, one registered provider |
| Non-loopback product bind | Not observed |
| Direct Tailnet `http://100.108.132.109:8787` | Timeout from peer (expected; Worker not on tailnet IP) |

`keryx doctor` (with env loaded): all required checks passed; providers `["openai_codex"]`; Telegram Gateway configured; Docker available; no Soul path; no MCP config; live health OK.

---

## 6. Tailnet Edge readiness

### 6.1 Topology (live)

```text
Mac / phone / work machine  --Tailscale-->  Caddy on 100.108.132.109:8443
                                              (and T3 on :443, CLIProxy on :8450)
                                                    |
                                                    v
                                         Worker 127.0.0.1:8787
```

| Layer | Live fact |
|-------|-----------|
| Tailscale | Host online; MagicDNS `jack-agent-worker.tail68a74b.ts.net`. Devices on same tailnet: `jack-work-machine` (linux), `jacks-macbook-pro` (macOS), `jack-android-phone` (android). |
| Edge process | systemd `jack-keryx-tailnet-proxy.service` (enabled, active since 2026-07-27). **Not** stock `caddy.service` (that unit is **masked**). Sibling private proxies: T3 `:443`, CLIProxy `:8450`. |
| Caddyfile | `/etc/caddy/jack-keryx-tailnet.Caddyfile`: `https://jack-agent-worker.tail68a74b.ts.net:8443` with `bind 100.108.132.109 [fd7a:…8470]`, `reverse_proxy 127.0.0.1:8787`, `admin off`, `auto_https disable_redirects`. |
| UFW | Active; allows `8443/tcp` (and 443, 8450, 22) **on `tailscale0` only**. |
| Funnel / Serve | `tailscale serve` / `funnel`: no config. |
| On-host Edge | `GET https://…:8443/health` via Tailscale IP + correct SNI → `{"status":"ok"}`. Unauthenticated `POST /v1/sessions` over Edge → **401**. Authenticated `/v1/providers` over Edge → **200**. |
| From `jack-work-machine` | TCP **443** succeeds (returns T3 web app HTML on `/health`, not Keryx). TCP **8443** and **8450** **time out**. So L5 “health over HTTPS from other machines” is **not verified** for the Keryx port from this peer. |
| Mac / phone | Present in `tailscale status`. **No** curl/Console check from those devices in this research. |
| Docs default hostname | Docs example uses `keryx.your-tailnet.ts.net` on default HTTPS. Live uses **same MagicDNS name as the host** on **port 8443** (because `:443` is occupied by T3). Operators must use `https://jack-agent-worker.tail68a74b.ts.net:8443`, not plain `:443`. |

### 6.2 Edge gaps

1. **Remote reachability of `:8443` from at least one tailnet client failed** (`jack-work-machine`). UFW and listen state look correct on the host; on-host proxy works. Root cause not fully isolated (possible client path, ACL elsewhere, or path-specific filter). **Blocks confident L5 sign-off.**
2. **Port/hostname non-default** vs `docs/deploy/tailnet-edge.md` example — easy operator footgun (hitting T3 on 443).
3. Caddy for Keryx is a **host-local custom unit**, not installed by `scripts/install.sh` / repo `deploy/Caddyfile.example` alone — deploy knowledge lives partly outside the monorepo.

---

## 7. Model / daily Run readiness

| Item | Fact |
|------|------|
| Default provider | `openai_codex` |
| Platform API keys | `OPENAI_API_KEY` / `XAI_API_KEY` **absent** in env |
| Codex token files | `~/.config/keryx/chatgpt-access-token` and `chatgpt-account-id` present (mode 600), mtime 2026-07-28 |
| JWT `exp` | **2026-07-30T13:39:33Z** — **expired** at probe (~7 days) |
| Diagnostic Run | Session+Run created on loopback; Run **failed**: `openai_codex: session expired or rejected (HTTP 401 Unauthorized)` |

**Impact:** Control plane and Edge can accept work, but **paid/subscription model path is dead** until `codex login` + sync (unit already has `ExecStartPre` sync — re-login / re-sync required). No secondary official API key configured as fallback.

Unit has not been restarted since 2026-07-28; pre-start sync alone will not refresh a long-running process’s in-memory token without restart after a successful sync.

---

## 8. Telegram Gateway posture

| Item | Fact |
|------|------|
| Token | `KERYX_TELEGRAM_BOT_TOKEN` **set** (length observed; **value not recorded here**) |
| Allowlist | `KERYX_TELEGRAM_ALLOWED_CHAT_IDS` **set** (1 id) — good fail-closed posture vs empty allowlist |
| Timeout | `KERYX_TELEGRAM_RUN_TIMEOUT_SECS=180` |
| Doctor | Telegram Gateway check **ok** |
| Live evidence | Journal shows at least one successful `telegram gateway started Run` (2026-07-30). Also intermittent `getUpdates failed; retrying` warnings (Jul 30, Aug 5). |
| Security gap | Journal log lines for getUpdates failures **embed the full bot token in the request URL**. That is a **secret leak into logs**. Treat token as **compromised for ops hygiene**; **rotate via BotFather** and scrub journals if retained. **Do not** paste tokens into issues/docs. |

Telegram is configured for away-from-desk use, but model expiry also blocks useful Telegram Runs.

---

## 9. Mac / phone reachability facts

| Device (Tailscale) | On tailnet? | Keryx Edge `:8443` proven? |
|--------------------|-------------|----------------------------|
| `jack-work-machine` | Yes (probe source) | **No** — connect timeout |
| `jacks-macbook-pro` | Yes (status) | **Not tested** |
| `jack-android-phone` | Yes (status) | **Not tested** |

Tailnet membership is necessary but not sufficient. Until `:8443` works from a real client, **Mac + phone daily Console over Edge is not proven**.

---

## 10. Config posture (non-secret)

Present and set (values only where non-secret):

- `KERYX_BIND=127.0.0.1:8787`
- `KERYX_DATA_DIR=/home/jacksiow/.local/share/keryx`
- `KERYX_DEFAULT_PROVIDER=openai_codex`
- `KERYX_GLOBAL_ACTIVE_CAP=2`
- `KERYX_OPERATOR_PRINCIPAL=operator`
- `KERYX_OPERATOR_TOKEN` set (long; file mode 600)
- `KERYX_WORKSPACE_ROOTS=…/workspace`
- Broad `KERYX_ALLOWED_TOOLS` including workspace FS, web, memory, `run_terminal`
- `KERYX_SKILLS_ROOT=…/skills` (directory empty)
- Codex file paths + `CHATGPT_CODEX_MODEL=gpt-5.6-sol`
- Telegram token + allowlist + timeout

Absent / soft-missing:

- `KERYX_SOUL_PATH`
- `KERYX_CONTEXT_FILES`
- `KERYX_MCP_CONFIG`
- Official `OPENAI_API_KEY` / `XAI_API_KEY`
- `GROK_WEB_COOKIE_FILE`

Env file mode **600** — good.

---

## 11. Gaps that block “I live on this host daily”

Ordered by daily-use impact:

1. **Model auth dead** — sole provider `openai_codex` JWT expired; diagnostic Run failed. Refresh Codex OAuth (or add official API key) and **restart** Worker.
2. **L5 client path unproven / broken from work machine** — Keryx HTTPS is on **:8443**; peer timeout. Fix or explain network path; verify from Mac and phone.
3. **Operator URL confusion** — host MagicDNS `:443` is **T3**, not Keryx. Document/bookmark `https://jack-agent-worker.tail68a74b.ts.net:8443`.
4. **Telegram bot token in journal URLs** — rotate token; fix logging to redact secrets (code follow-up outside this research).
5. **Stale binary vs current `main`** — host build ~2026-07-28; daily-use Console/API surface may need a deliberate upgrade + smoke.
6. **Empty Soul / skills / workspace** — doctor soft-ok, but thin for “agent OS” daily bar (product map #64; not pure L4/L5 infra).
7. **Reboot proof** — Linger + enable look correct; no controlled reboot test in this research.
8. **Docs path drift** — L4 docs emphasize `--system` + `/var/lib/keryx`; live is user install. Acceptable if intentional; checklist should treat user+linger as valid L4 or operators will “fix” green installs.

Non-blocking / healthy:

- Loopback-only Worker, bearer auth enforced
- SQLite data dir writable and in use
- Edge unit enabled, Tailscale bind only, no Funnel
- Telegram allowlist set
- Docker reported available for reduced-origin exec

---

## 12. What could not be verified

| Item | Why |
|------|-----|
| Controlled reboot → `systemctl --user is-active keryx` | Would disturb production host |
| Mac / phone Console or curl to `:8443` | No interactive session on those devices |
| Root cause of work-machine `:8443` timeout | Needs deeper network/ACL debug; host UFW/listen OK |
| SQLite row-level integrity / post-restart Session identity | No `sqlite3` CLI; no Worker restart |
| End-to-end SSE Run over Tailnet with live model | Model token expired; remote Edge path failed from probe client |
| Whether `sync-chatgpt-token.sh` can refresh without re-login | Not exercised beyond reading unit |

---

## 13. Minimal operator actions to close L4/L5 for daily use

No production changes made by this research. Suggested sequence for a human operator:

1. Refresh Codex: `codex login` (or equivalent), run sync script, **restart** user unit: `systemctl --user restart keryx`.
2. Confirm `keryx doctor` and `./scripts/smoke.sh` on host loopback (L2–L3).
3. From Mac (and phone if possible):  
   `curl -sS https://jack-agent-worker.tail68a74b.ts.net:8443/health`  
   then authenticated Session + Run (L5).
4. If Mac works but work-machine fails, treat as client/path ACL issue; if both fail, debug Edge/UFW/Tailscale on host with care.
5. Rotate Telegram bot token; redeploy env; restart Worker; consider log redaction fix in gateway code.
6. Plan binary upgrade from current monorepo when daily-use features require it.
7. Optional: document host-specific Edge URL/port in operator notes; optional system install only if user+linger is rejected as L4.

---

## 14. Readiness score (research opinion)

| Bar | Status |
|-----|--------|
| L4 always-on Worker | **Mostly ready** (user systemd + linger + health + SQLite). Reboot test and model auth still open. |
| L5 Tailnet Edge | **Infra present, client path incomplete**. Edge correct on paper and on-host; remote Keryx HTTPS not proven from probe peer; Mac/phone untested. |
| Daily-use on this host | **Not ready** until model token is live **and** at least one remote client completes health + auth Run over `:8443`. |

---

## 15. References

- `docs/deploy/operator-checklist.md` — levels 4–5
- `docs/deploy/install.md` — user vs system paths
- `docs/deploy/tailnet-edge.md` — Edge rules
- `docs/deploy/v2-agent-os.md` — Telegram env
- `deploy/keryx.service` — system unit template
- `deploy/Caddyfile.example` — example Edge
- ADR 0003 (loopback + Tailnet edge), ADR 0004 (operator token), ADR 0006 (SQLite)

---

## Appendix A — Redaction note

This document deliberately omits: operator bearer token, Telegram bot token, Codex JWT, API keys, and full Session/Run payloads. Journal evidence shows Telegram tokens can appear in URLs; do not copy journal lines into tickets without redaction.
