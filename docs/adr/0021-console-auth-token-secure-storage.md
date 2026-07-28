# Console v1 auth is operator token in secure storage

Console authenticates as today’s Principal: Worker base URL + bearer operator token, stored in OS secure storage (Keychain/Keystore), optional biometric app lock. Reachability stays Tailnet/Edge out-of-band; Console is not a VPN or IdP client. Push payloads never carry the token. API client types should allow per-device/scoped tokens later without rewriting the shell. Rejected for v1: OAuth/SSO (multi-tenant overkill) and blocking first ship on device-paired token issuance (desirable evolution, not a gate).
