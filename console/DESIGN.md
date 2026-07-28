# Keryx Console Design System

Register: **product** (operator tool; design serves the task).  
Aligned with ADR 0031 (messaging chat-list IA), ADR 0032 (messenger principles, not skin), and impeccable product laws.

## Scene

Principal operator at a desk (or phone in motion) over Tailnet: high-trust **messenger for an agent**—chat list, open thread, Approvals that cannot be missed, tool-heavy Runs that stay readable. Ambient light is normal office or evening desk. Mood: calm authority, not consumer chat marketing, not IDE density theater.

## Color strategy: Restrained

Tinted neutrals toward a cool slate hue + one **needs-you** accent ≤10% surface area.

| Token | Light (approx OKLCH) | Role |
|-------|----------------------|------|
| `canvas` | cool slate 98% | App background |
| `panel` | cool slate 96% | Chat list / side panels |
| `surface` | cool slate 99% | Open thread surface |
| `ink` | cool slate 18% | Primary text |
| `inkMuted` | cool slate 45% | Secondary text, previews |
| `line` | cool slate 88% | Hairline dividers |
| `accent` | blue 0.55 L, moderate chroma | Primary actions, selection, Send |
| `needsYou` | ember 0.62 L | Approvals, badges, attention only |
| `ok` / `warn` / `err` | semantic | Run / connectivity states |

Never pure black or pure white. Needs-you is never used as page chrome fill.

## Typography

System stack (native feel on macOS/iOS). Fixed scale ratio ~1.2.

| Step | Use |
|------|-----|
| label (11–12) | Meta, badges, timestamps, activity summaries |
| body (14) | Chat prose, forms |
| titleSm (13–14 w600) | List section headers, system row titles |
| titleMd (16–17 w600) | Thread header (agent + Session title) |
| titleLg (20–22 w600) | Onboarding brand mark |

## Spacing

4px base. Rhythm: 8 / 12 / 16 / 24. Chat list: denser (8–12). Thread: more air (12–16). Composer dock: 12–16 padding.

## Elevation

Almost flat. One soft elevation on composer dock, sticky Approval card, and dialogs only. No nested cards in lists.

## Information architecture

| Surface | Role |
|---------|------|
| **Chat list** | Sessions as rows (title, last preview, attention badge, Active Run hint) + thin **Needs you** system row |
| **Thread** | Layered timeline: Principal/agent prose messages; collapsible tool/Child Run/status activity; sticky Approval when relevant |
| **Composer** | Idle: Send starts root Run. Active: wait / cancel / cancel-and-re-run (no silent second root) |
| **Session info** | Contextual pane or screen: Policy, Workspace, title |
| **Profile hub** | Memory, Skills, Schedules, Settings, connectivity |
| **New chat** | Empty Session under defaults; first Send starts first Run—no mandatory wizard |

## Layout breakpoints

| Width | Layout |
|-------|--------|
| ≥1100 | Chat list \| open thread; optional third pane only for Session info / artifact |
| 720–1099 | List *or* thread (push navigation) |
| <720 | Full-screen list → full-screen thread; hub via avatar/menu |

No permanent dual rail (Inbox + Sessions). Needs you is a list row, not a peer column.

## Components

- **Chat list row**: full-width; selected = filled panel tint + weight, not left stripe. System rows (Needs you) may use needs-you badge count.
- **Message**: first-class prose for Principal and agent; clear author/time. Prefer readable message rows over consumer bubble cosplay when density helps operators.
- **Activity block**: collapsed summary (tool count, status); expand in place; monospaced summary optional.
- **Sticky Approval**: above composer; Approve uses needs-you fill; Deny secondary.
- **Badge**: pill for counts; solid needs-you fill only for attention counts.
- **Empty state**: icon + one sentence + optional single action (e.g. new chat / open Session info).
- **Hub pages** (Memory, Schedules, Skills, Settings): list-row + dock pattern; no nested cards.
- **App chrome**: Worker connectivity in profile/header; secondary tools under menu (labels, not icon-only cluster).

## Motion

150–220ms ease-out on selection, expand, and list↔thread transitions. No page-load choreography.

## Shared widgets

`console/app/lib/widgets/console_chrome.dart`: `ConsoleEmptyState`, `AttentionBadge`, `RailSectionHeader`, `StatusPill`, `ConsoleBanner`, `ConsoleLoader`, `ConsolePageScaffold`, `ConsoleListRow`, `ConsoleSectionLabel` (rename/extend toward chat-list vocabulary as implementation lands).
