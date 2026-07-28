# Console lives in the Keryx monorepo behind an OpenAPI seam

Flutter Console lives under `console/` in this repository beside `crates/`. Coupling to the Worker is a checked-in HTTP contract (OpenAPI or equivalent under `docs/api/`), not Rust FFI and not a separate product repo. Enables big-bang 1.0 API+UI changes in one PR while preserving hexagonal boundaries. CI should path-filter Rust vs Flutter and fail on contract drift between OpenAPI and the Dart API package.
