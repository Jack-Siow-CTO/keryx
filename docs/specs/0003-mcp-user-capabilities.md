# Keryx — User-added capabilities via MCP client Tools

Status: **ready-for-agent**  
Aligned with: `CONTEXT.md`, ADRs 0001–0012 (especially 0005, 0008, 0009, 0011, 0012), grill shared understanding 2026-07-27  
Test seams: **(1)** Control plane in-process extended for MCP under Policy/Approval, **(2)** Model provider contracts for native tool catalog + structured tool_calls  
Refines: MCP client stories in spec 0002 (does not replace v2; narrows *how* operators add long-tail Tools)  
Does not supersede: MCP **server export** mode from spec 0002 (still in v2; out of *this* slice’s ship bar unless noted)

---

## Problem Statement

I run a personal Keryx **Worker** (for example on `jack-agent-worker`). I want the agent to act in systems like Gmail, Slack APIs, Home Assistant, and other long-tail products—search mail, draft/send under control, post or read channels—**without** forking the core for every SaaS and **without** turning Keryx into a plugin marketplace or dynamic native loader.

Today:

- **Tool** execution is Policy-gated and fail-closed, which is correct.
- MCP exists only as **mock peer** behavior for Seam 1; real servers are not operator-configurable end-to-end.
- **Model providers** largely do not receive a first-class tool catalog or reliably return structured tool calls, so even registered tools are hard for the model to use deliberately.
- Product language drifts toward “plugins/capabilities,” but the glossary already distinguishes **Tool**, **Skill**, **Gateway**, and reserves “plugin” for dynamic native loading we rejected.

Without a clear path:

- Every integration becomes a first-party feature request against core, or
- I paste secrets into prompts / Memory, or
- Messaging Gateways get confused with API Tools (Slack-as-chat vs Slack-as-tools).

I need **operator-easy extension**: declare an **MCP server**, own its credentials, allowlist exact tool names under **Policy**, optionally mark high-blast tools for **Approval**, restart, and have control-plane **Runs** invoke those Tools safely.

---

## Solution

Ship **user-added capabilities as MCP client Tools** on the existing Worker spine.

From my perspective:

1. I add an **MCP server** in **static Worker config** (command/URL, secret refs, stable server id)—not a dynamic plugin, not a control-plane install API in this slice, not agent self-install.
2. After **restart**, the Worker connects (stdio and/or remote HTTP/SSE per server), discovers tools, and registers them as **`mcp.<server_id>.<tool_name>`**.
3. **Connect ≠ allow.** Tools remain uninvocable until each name is on the Run’s **Policy** allowlist. **Gateway** and **Schedule** origins get **no MCP tools by default**.
4. Credentials stay **operator-owned** (env / secret files / refs). OAuth consent happens outside Keryx with the MCP server’s own setup. Secrets never live in Memory, Transcript, or Approval payloads beyond redaction.
5. Outbound/destructive tools I mark in config require **Approval** using the existing Approval control plane (CLI/TUI/API).
6. The **agent loop** advertises only the **tool catalog** = registered ∩ Policy (with JSON schemas from MCP and first-party tools) to the **Model provider** via native function/tool calling, then executes structured tool calls under the same gates as today.
7. Disconnect or MCP failure **fails closed** for invokes. `keryx doctor` shows MCP connection health and discovered/allowlisted names (not secret values).
8. Docs give recipes (e.g. Gmail-class MCP): secret file → config block → Policy allowlist → high-blast flags → restart → doctor → control-plane Run.

Long-tail product integrations enter **only** this way. First-party core keeps **classes** of power (files, terminal, browser, Memory, …), not Gmail/Slack product APIs. **Gateway** remains for chat surfaces; this spec is not Slack-as-Gateway.

---

## User Stories

### Operator setup and configuration

1. As an operator, I want to declare one or more **MCP servers** in static Worker configuration, so that I can add external Tools without recompiling Keryx for each product.
2. As an operator, I want each MCP server to have a stable **server id** I choose, so that tool names and Policy entries remain stable across restarts.
3. As an operator, I want invalid server ids rejected at startup (stable, restricted character set), so that namespaces stay predictable.
4. As an operator, I want to configure **stdio** MCP servers (command, args, working directory, env), so that local MCP processes on the Worker host work.
5. As an operator, I want to configure **remote** MCP servers (URL + auth ref), so that hosted MCP endpoints work without local spawn.
6. As an operator, I want transport chosen **per server**, so that mixed local and remote MCP estates are possible.
7. As an operator, I want optional Docker (or other isolation) as a **per-server choice**, not a mandatory wrapper for every MCP server, so that token files and simple stdio servers stay usable.
8. As an operator, I want MCP credentials referenced via env vars or secret files (mode 600), so that secrets are not committed to git.
9. As an operator, I want raw OAuth refresh tokens never required to be pasted into Soul, Skills, or chat, so that agent-visible state is not a credential store.
10. As an operator, I want changing MCP config to take effect after **Worker restart**, so that apply semantics stay simple and correct.
11. As an operator, I want config parsing kept pure (config → client set) so a future hot reload is possible without redesign, so that restart-only ops do not paint us into a corner.
12. As an operator, I want a documented example config for at least one mail-class and one messaging-API-class MCP server, so that “easy add” is a recipe, not tribal knowledge.
13. As an operator, I want misconfiguration (missing binary, bad URL, missing secret file) to fail closed with clear doctor/startup diagnostics, so that silent half-connects do not look like success.
14. As an operator, I want Worker shutdown to tear down stdio MCP children cleanly, so that orphan processes do not accumulate.

### Discovery, naming, and catalog

15. As a Principal, I want discovered MCP tools named **`mcp.<server_id>.<tool_name>`**, so that Policy, Transcript, and logs never collide across servers.
16. As a Principal, I want renaming a server id to be an explicit break (new tool names), so that Policy cannot silently point at the wrong peer.
17. As a Principal, I want MCP tool descriptions and JSON schemas preserved for the model catalog, so that the agent can call tools correctly.
18. As a Principal, I want the invocable catalog for a Run to be the intersection of registered tools and Policy allowlist, so that the model never sees tools it cannot use.
19. As a Principal, I want first-party Tools and MCP Tools to share the same catalog and invoke path, so that the agent loop does not special-case “plugins.”
20. As an operator, I want doctor (or equivalent) to list connected MCP servers and discovered tool names, so that I can verify setup before trusting a Run.
21. As an operator, I want doctor to distinguish “connected but not allowlisted” from “allowlisted and invocable under control_plane Policy,” so that connect ≠ allow is visible.
22. As a Principal, I want unknown or undiscovered MCP tool names denied, so that stale Policy entries fail closed rather than hanging.

### Policy and Run origin

23. As a Principal, I want default-deny for MCP tools not on the allowlist, so that enabling a server is not a Policy bypass.
24. As a Principal, I want control-plane Runs to use only MCP tools I explicitly allowlisted, so that trust stays deliberate.
25. As a Principal, I want **gateway:*** and **schedule** origins to have **no MCP tools by default**, so that Telegram/Discord/cron cannot send mail or post externally without an explicit future Policy choice.
26. As a Principal, I want Child Runs to inherit only a Policy subset of the parent, so that delegated work cannot gain MCP tools the parent lacked.
27. As a Principal, I want Policy allowlists to use exact MCP tool names (not “all tools from server” by default), so that a server that grows new tools does not auto-expand blast radius.
28. As an operator, I want optional documentation for carefully allowlisting read-only MCP tools on reduced origins later, so that a future widen is possible without redesigning namespaces—but that widen is not the default of this spec.
29. As a Principal, I want tool denials to surface as tool failures with clear reasons in Transcript/events, so that the agent and I can see Policy in action.
30. As a developer, I want origin Policy templates and operator registration allowlists both enforced, so that neither env registration nor domain Policy alone is a single point of failure.

### Approval and high-blast MCP

31. As an operator, I want to mark specific MCP tools (or explicit config patterns) as requiring **Approval**, so that send/delete/public-post class actions wait for me.
32. As a Principal, I want high-blast MCP invokes to create durable Approval records with redacted summaries, so that I can approve/deny from control plane clients.
33. As a Principal, I want deny or timeout of Approval to fail the tool call closed, so that silence is not success.
34. As a Principal, I want Approval attribution to Principal, so that audit of who allowed an MCP action is preserved.
35. As a Principal, I want non-high-blast allowlisted MCP tools (e.g. search/list) to run without Approval, so that multi-step triage stays usable.
36. As an operator, I want no automatic high-blast heuristics based on tool name substrings, so that foreign MCP naming schemes do not falsely gate or skip gates.
37. As a Principal, I want Approval summaries never to include secret tokens or full message bodies beyond safe truncation/redaction policy, so that the Approval list is not a leak channel.

### Agent loop and model providers

38. As a Principal, I want the agent loop to pass the Policy-filtered tool catalog into Model provider completions, so that models can request structured tool calls.
39. As a Principal, I want Model providers that support native tools/functions to send schemas on the wire, so that tool use is reliable compared to prompt-stuffed lists.
40. As a Principal, I want structured tool_calls from the model to map to the same Tool invoke path as first-party tools, so that MCP and core tools share enforcement.
41. As a Principal, I want tool call budgets to count MCP invokes, so that runaway external API use is bounded like other tools.
42. As a Principal, I want cancel of an Active Run to stop waiting MCP calls where the transport allows, so that cancel remains meaningful.
43. As a Principal, I want MCP tool results appended to Transcript for continuity, so that multi-step external work survives across model steps.
44. As a Principal, I want Run events for tool started/finished to summarize MCP args/results without secrets, so that SSE clients stay safe.
45. As a developer, I want Seam 2 fixtures to prove catalog serialization and tool_call parsing without live model network, so that merge CI stays deterministic.

### Runtime reliability and security

46. As a Principal, I want MCP disconnect mid-call to fail the tool invocation closed, so that hangs are visible failures.
47. As a Principal, I want subsequent calls against a dead MCP server to fail closed until restart/reconnect policy applies, so that half-dead state is not success.
48. As an operator, I want reconnect behavior to avoid restart storms (backoff or restart-only in this slice), so that a crashing MCP binary cannot DOS the Worker.
49. As a Principal, I want size/time limits on MCP tool results, so that huge payloads cannot blow Transcript, events, or memory.
50. As a Principal, I want secret-like keys redacted in MCP tool argument event summaries, so that tokens in args are not streamed.
51. As an operator, I want stdio MCP children to run as the Worker user by default (no implicit root helper), so that process privilege matches the Worker.
52. As an operator, I want Tailscale membership never treated as authorization for MCP or control plane, so that mesh reachability stays separate from Principal auth.
53. As an operator, I want the control plane to remain loopback-bound with operator token auth, so that adding MCP does not open a public tool gateway.

### Skills, Memory, and non-confusion with other concepts

54. As a Principal, I want Skills to remain documents that may *describe* how to use MCP tools, so that knowledge and executable power stay separate.
55. As a Principal, I want Skills never to grant MCP invoke rights by themselves, so that loading a skill is not a Policy bypass.
56. As a Principal, I want Memory never to be the store for OAuth tokens for MCP, so that curated facts are not credentials.
57. As an operator, I want Slack/Telegram **as chat** to remain Gateway work, so that this MCP path is not misused as a second messaging runtime.
58. As a developer, I want glossary terms Tool / MCP server / Skill / Gateway / Policy / Approval used in code and docs, so that “plugin/capability pack” language does not re-enter the domain model.

### Operator clients and observability

59. As a Principal, I want existing Approval CLI/API to work for MCP high-blast without a parallel approval system, so that ops stay one queue.
60. As a Principal, I want Run event streams to show MCP tool phases like other tools, so that progress is visible.
61. As an operator, I want `keryx doctor` to report MCP readiness alongside providers/Soul/skills, so that deploy mistakes are obvious.
62. As an operator, I want logs to name server id and tool name on MCP failures without dumping secrets, so that debugging is possible on the worker host.

### Explicit non-goals as stories (acceptance of boundaries)

63. As an operator, I accept that this slice does not ship a first-party Gmail or Slack Rust tool crate, so that core does not become a SaaS zoo.
64. As an operator, I accept that dynamic `.so`/script plugins are out of scope, so that packaging and security taxes stay deferred.
65. As an operator, I accept that control-plane CRUD to add MCP at runtime is out of this slice, so that static config + restart remains the supported path.
66. As an operator, I accept that the agent cannot install MCP servers mid-Run, so that self-escalation of tools is impossible by design.
67. As an operator, I accept that Keryx does not run a product OAuth broker in this slice, so that token UX stays with operator + MCP server docs.
68. As an operator, I accept that MCP **server export** (Keryx serving tools to other agents) remains covered by v2 generally and is not the ship bar of this slice, so that client-ingest is prioritized.
69. As a developer, I want CI free of live Gmail/Slack/OAuth network calls, so that merges do not depend on third-party SaaS.

### Developer and architecture

70. As a developer, I want MCP client behavior behind the same Tool ports as other adapters, so that the agent loop stays free of MCP protocol details.
71. As a developer, I want hexagonal direction preserved (domain ← app ← adapters ← worker), so that MCP SDKs do not leak into domain.
72. As a developer, I want compile-in registries for MCP client wiring, so that extension stays consistent with ADR 0005/0012.
73. As a developer, I want Seam 1 to be the primary correctness surface for Policy/Approval/disconnect, so that most regressions are caught without live peers.
74. As a developer, I want Seam 2 to cover catalog-on-the-wire and tool_call parse contracts, so that model adapters cannot silently drop tools.
75. As a future implementer, I want pure config→clients construction, so that a later reload signal or control-plane registration API can reuse the same builder.

---

## Implementation Decisions

These decisions encode the 2026-07-27 grill shared understanding and ADRs 0001–0012. Vocabulary matches `CONTEXT.md`.

### Product shape

- User-added long-tail product integrations are **MCP client Tools**, not first-party product adapters, not Gateways, not Skills, not dynamic plugins.
- **Gateway** remains messaging-only; Slack-as-chat is out of this spec’s delivery (may already be future Gateway work under v2).
- **Skill** may document MCP usage; never authorizes Tools.
- Single-operator **Principal** model unchanged; multi-tenant capability marketplaces out.
- Control plane remains system of record for Sessions, Runs, Approvals, events; MCP does not introduce a parallel agent runtime.

### Registration and lifecycle

- **Static configuration only** for this slice: declare MCP servers in Worker config / env / config file read at process start.
- Supported apply path: edit config → **restart Worker**.
- Implementation must keep “parse config → build MCP clients → register tools” as a pure composition step so hot reload or future control-plane CRUD is not a rewrite; **hot reload is not required to ship**.
- **Agent self-install** of MCP servers is denied by design (no tool that mutates MCP config from a Run).
- Startup connects to configured servers, lists tools, registers namespaced Tools; partial failure policy: fail closed per server (do not pretend tools exist), surface via doctor/logs; Worker may still start if other subsystems healthy unless operator chooses strict mode (default: Worker starts; broken MCP server contributes zero tools and fails invokes).

### Transports

- Support **stdio subprocess** and **remote HTTP/SSE** (or current MCP remote transport equivalent) selectable per server.
- Stdio is the default recommendation for on-host personal setup.
- Optional container isolation is per-server operator choice, not global mandatory.
- Stdio children default to Worker user privileges; no implicit privilege escalation helper.
- On Worker shutdown, abort in-flight MCP calls and terminate owned children.

### Naming

- Every MCP tool name is **`mcp.<server_id>.<tool_name>`**.
- `server_id`: operator-chosen, stable, restricted charset (e.g. lowercase alphanumeric + underscore).
- No flat names, no collision-dependent renaming, no core alias map in this slice.
- Changing `server_id` renames all tools; Policy must be updated explicitly.

### Policy

- Fail closed: unknown tools denied.
- **Discover/connect does not allowlist.**
- Policy allowlists use **exact** tool names for MCP (same mechanism as first-party tools).
- Default **reduced** origin templates (`gateway:*`, `schedule`) include **no** MCP tool names.
- Default **control_plane** template does **not** auto-include all discovered MCP tools; operator must add names (via Policy template config / allowed-tools configuration consistent with existing allowlist mechanisms).
- Child Run Policy is a subset of parent; cannot gain MCP tools parent lacks.
- Both “tools registered in the runtime” and “Policy.allows_tool” gates remain; registration alone is insufficient.

### Approval

- Reuse existing **Approval** aggregate and control-plane list/approve/deny.
- High-blast MCP tools are **config-declared** (per tool name and/or explicit server-level list)—no substring heuristics.
- Recommended docs posture: mark outbound/destructive tools high-blast; leave read/search allowlist-only when safe.
- High-blast path: create pending Approval, redacted summary, wait or fail closed on deny/cancel; attribute decision to Principal.
- Non-high-blast allowlisted MCP tools invoke without Approval.

### Secrets

- Operator-owned only: env, secret files, refs in config.
- No Keryx OAuth broker / token vault product in this slice.
- No storage of MCP credentials in Memory, Transcript, Soul, or Skill bodies as a supported pattern.
- Event and Approval summaries redact secret-like keys and truncate large bodies; strip URL userinfo as with existing tool arg summarization spirit.

### Tool catalog and model providers

- Introduce a first-class **tool catalog** concept at the app/tool port boundary: name, description, parameters schema.
- For each model step, catalog = tools registered for the Worker ∩ Policy for that Run’s origin (and any tighter session/run constraints if present).
- Model providers that can express tools must send the catalog using native tool/function calling APIs.
- Prompt-stuffed freeform tool lists are **not** the primary mechanism.
- Providers must parse structured tool calls into the agent loop’s tool invocation type; empty tool_calls when the model requested tools is a contract failure for Seam 2 fixtures.
- MCP JSON schemas from `tools/list` feed the catalog; first-party tools supply equivalent schemas.
- Budget counters (tool calls, time, tokens) apply unchanged in spirit; MCP calls consume tool-call budget.

### MCP client behavior

- List tools after connect; call tool by namespaced name mapping to peer’s local name.
- Disconnect / transport error → tool failure (fail closed), not infinite hang.
- Result size and time limits enforced at adapter boundary.
- Mock MCP peer remains for CI; live third-party SaaS is not a merge gate.
- MCP protocol details stay in the tools/mcp adapter area; domain knows Tool names and Policy only.

### MCP server export (scope boundary)

- Spec 0002 still envisions Keryx as MCP server exporting selected tools under Principal auth.
- **This slice prioritizes MCP client ingest.** Server export may remain stub/mock or follow-on; it must not block client delivery, and must not weaken auth if touched.

### Configuration surface (behavioral)

Minimum operator-configurable fields per MCP server:

- `server_id`
- transport (`stdio` | remote)
- command/args **or** URL
- env / secret file refs
- optional: enabled flag, cwd, timeouts
- optional: tool allowlist filter at registration time (narrow which discovered tools are even registered)
- optional: high-blast tool name list
- optional: isolation notes (e.g. dockerized command)

Worker-level:

- path or structured env pointing at MCP config
- interaction with existing global tool registration allowlists must not silently register disallowed classes

Doctor reports:

- each server: configured / connected / error
- discovered tool names (namespaced)
- which are present on default control_plane Policy template if knowable
- never secret values

### Module responsibilities (hexagonal)

| Area | Responsibility |
|------|----------------|
| domain | Policy allow/deny, origin templates, Approval rules as pure data; no MCP SDK |
| app | Agent loop: catalog for Run, Policy check, Approval gate, budgets, transcript/events; ports for tool catalog + invoke |
| tools / mcp adapter | Connect, list, call, disconnect, schema mapping, redaction helpers, mock peer |
| model | Serialize catalog to provider wire format; parse structured tool_calls |
| worker | Read static config, compose clients, register tools, doctor, lifecycle/shutdown |
| api / CLI | Existing Approvals + doctor; no required new CRUD for MCP registration in this slice |
| storage | No credential store for MCP; Approval durability unchanged in spirit |

Rules:

- Domain has no MCP protocol types.
- App depends on ports, not concrete MCP transport clients.
- Adapters compile in via registries; no dynamic native plugins.
- Fail closed on auth, Policy deny, Approval deny, disconnect.

### Relationship to existing ADRs / specs

- **ADR 0005 / 0012:** compile-in adapters; long-tail via MCP/Skills not first-party zoo; no dynamic plugins — this spec is the detailed client path.
- **ADR 0008:** hexagonal workspace — MCP is an adapter area.
- **ADR 0009:** CI without live third-party; Seam 1 + Seam 2 gates.
- **ADR 0011:** provider registry — catalog must flow through provider completions.
- **Spec 0002:** MCP stories 95–99 refined here for client; server export deferred as ship bar for this slice.
- **CONTEXT.md:** Tool, MCP server, Skill, Policy, Approval, Gateway definitions are normative language.

### Suggested implementation order (non-binding)

1. Tool catalog port + Policy intersection in agent loop  
2. Model provider catalog send + structured tool_call parse (Seam 2 fixtures)  
3. Static MCP config parse + composition root wiring  
4. Real MCP client (stdio first, then remote) behind Tool invoke + list  
5. Approval flags from config into existing high-blast gate  
6. Origin defaults: no MCP on reduced Policy templates  
7. Doctor + operator docs/recipes  
8. Seam 1 expansion replacing/extending mock-only guarantees with config-driven mock peer  

---

## Testing Decisions

### What good tests look like

- Assert **external behavior**: Policy deny/allow, Approval required/denied, namespaced names, catalog membership, fail-closed disconnect, redacted summaries, provider request contains tools, parsed tool_calls invoke the tool path.
- Do **not** lock tests to MCP SDK internals, exact log strings, or private struct layouts.
- No live Gmail/Slack/OAuth or paid third-party MCP in merge CI.
- Prefer highest seams; avoid a scatter of low-level tests that re-implement protocol coverage already mocked at Seam 1.

### Seams (agreed)

1. **Seam 1 — Control plane in-process (primary)**  
   Worker-shaped app with mock MCP peer and static config doubles:  
   - config → `mcp.<id>.*` registration  
   - connect ≠ allow  
   - reduced origin denies MCP by default  
   - high-blast config → Approval path  
   - disconnect fails invoke  
   - catalog ∩ Policy observable via deny vs successful invoke  
   - secrets absent from Approval/tool event summaries  

2. **Seam 2 — Model provider contracts (secondary)**  
   Fixture-based:  
   - request includes tool catalog (names + schemas) for allowed tools  
   - structured tool_calls parse into agent tool invocations  
   - no live network  

3. **Not required for this feature**  
   - Seam 3 Gateway contracts (unchanged)  
   - Live SaaS MCP  
   - Plugin loader tests  

### Modules under test

- Domain Policy origin templates (MCP absent from reduced defaults).
- App agent loop gating (Policy, Approval, budgets) with fake model emitting tool_calls for MCP names.
- MCP adapter (mock + client unit tests for namespace mapping and fail-closed).
- Model adapters (catalog serialization + tool_call parse fixtures).
- Worker config parse (invalid id, missing transport fields) as thin tests feeding Seam 1.

### Prior art in this repo

- Seam 1 MCP mock peer tests (namespaced tools, auth on server export mock, disconnect fail-closed).
- Seam 1 Approval / origin Policy / terminal high-blast patterns.
- Seam 2 model fixture tests (ADR 0009 / spec 0002).
- Composite tool router patterns for first-party tools.

### Explicit CI rules

- Default `cargo test` / merge gate: no live MCP network dependencies.
- Optional manual/operator verification doc: real stdio MCP binary on worker host (not merge-blocking).

---

## Out of Scope

- First-party Gmail, Slack, HA, Spotify (etc.) Rust tool implementations in core  
- Dynamic native/script **plugins** (`.so`, in-process script plugins)  
- Control-plane **CRUD** API to add/remove MCP servers at runtime (this slice)  
- **Hot reload** of MCP config without restart (design for purity only)  
- **Agent-driven** installation or mutation of MCP configuration  
- Keryx-managed **OAuth broker** / multi-account token vault product  
- MCP **server export** as ship bar for this slice (still a v2 concept; not required here)  
- **Gateway** for Slack/Gmail-as-chat  
- Skills Hub / plugin marketplace / curated one-click store client  
- Auto-allow all tools from an enabled server; name-based Approval heuristics  
- Multi-Principal tenancy and per-human capability ACLs  
- Mid-tool exactly-once resume across Worker crash  
- Guaranteeing third-party MCP server quality or security beyond Policy/Approval boundaries  
- Desktop app UX for adding integrations  

---

## Further Notes

### Access model reminder (operator expectations)

Adding MCP does not mean “full machine god mode.” File tools remain Workspace-jailed; terminal remains origin- and Approval-gated; MCP power equals **peer credentials × Policy allowlist × Approval flags**. Operators should still treat approved local terminal on a multi-purpose worker as high blast independent of MCP.

### “Easy” definition for this spec

Easy means: **static config + secret files + explicit allowlist + restart + doctor**, with docs recipes—not a marketplace, not in-band OAuth in the agent loop, not compile-per-SaaS.

### Glossary anchors

Use **Tool**, **MCP server**, **Policy**, **Approval**, **Run origin**, **Principal**, **Skill**, **Gateway** exactly as in `CONTEXT.md`. Avoid “plugin,” “capability pack,” and “integration” as domain nouns for this feature.

### Follow-on candidates (not this ship bar)

- Control-plane MCP registration API reusing pure config builder  
- Hot reload with bounded reconnect  
- MCP server export hardening  
- Optional prefix allowlist syntax (if exact-name lists become unwieldy)—only with explicit security review  
- Gateway origin carefully allowlisting read-only MCP tools  

### Status

Ready for agent implementation against Seam 1 + Seam 2 as defined above. Sequence may land as multiple PRs; behavior in this document is the acceptance bar for the feature slice.
