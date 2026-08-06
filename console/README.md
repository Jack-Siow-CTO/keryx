# Keryx Console

Flutter multi-platform **Console**: thin Principal client of the Worker control plane
(ADRs 0013–0034 messaging IA, spec `docs/specs/0004-console-1.0.md`).

## Layout

```
console/
  packages/keryx_api/   # pure Dart HTTP/SSE client (no Flutter)
  app/                  # Flutter UI + Riverpod
docs/api/openapi.yaml   # checked-in OpenAPI seam (ADR 0024)
```

## Prerequisites

- Flutter stable (≥ 3.24) with desktop/iOS toolchains as needed
- A running Keryx Worker (loopback or Tailnet Edge)

## Develop

```bash
# API client (no Flutter)
cd console/packages/keryx_api && dart pub get && dart test

# Console app
cd console/app && flutter pub get
flutter test
flutter run -d macos   # or ios
```

## Auth (ticket #38)

1. Enter Worker **base URL** + **operator token**
2. Token is stored in **OS secure storage** only (never SharedPreferences plaintext)
3. Optional **device unlock** (biometric / device credential) gates opening Console
4. **Check connectivity** distinguishes unreachable Worker vs auth failure
5. **Log out** deletes secret + local caches

Console does **not** hold model API keys, host an agent loop, or queue offline Runs.
