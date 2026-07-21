# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## What this is

`matrix-oracle` is a small (~1300 LOC) library crate that resolves Matrix
`.well-known` information: homeserver discovery for the client-server API and
delegated-server discovery for the server-server API. HTTP via `reqwest`
(+`reqwest-middleware`), DNS via `hickory-resolver`.

## Commands

```sh
cargo nextest run          # run tests (nextest is the configured runner; .config/nextest.toml)
cargo nextest run resolve  # run a single test by substring
cargo test                 # also works; runs doctests too
cargo clippy --all-targets # lints; config lives in lints.toml (see below)
cargo fmt                  # rustfmt.toml: hard tabs, width 100, StdExternalCrate imports
```

Feature-gated code must be checked under the right features:

```sh
cargo clippy --no-default-features --features client
cargo clippy --no-default-features --features server,rustls
```

## Architecture

Two independent resolvers, each behind a cargo feature (`client`, `server`, both
default-on). They share nothing but the caching middleware.

- `src/client.rs` — `client::Resolver::resolve(name)` implements the
  [client-server well-known URI] algorithm: GET `.well-known/matrix/client`,
  treat 404 as "use the name directly", then validate the discovered homeserver
  via `_matrix/client/versions` and (if present) the identity server at
  `/_matrix/identity/v2`. Error variants map directly to the spec's
  `FAIL_PROMPT` / `FAIL_ERROR` codes (`Error::Prompt` / `Error::Fail`).
- `src/server.rs` — `server::Resolver::resolve(name)` implements the 5-step
  [server-server resolution] algorithm (IP literal → host:port → `.well-known`
  delegation → SRV → bare host). The SRV step queries `_matrix-fed._tcp`
  (spec v1.8+) first, falling back to the deprecated `_matrix._tcp` record if
  that lookup comes up empty. The `Server` enum encodes which step won;
  `host_header()` and `address()` derive the connection details, and
  `socket()` does the final DNS `A`/`AAAA` lookup. The step-by-step `debug!`/
  `info!` tracing mirrors the spec — keep it in sync when changing logic.
- `src/lib.rs` — `cache()` builds the shared `http-cache-reqwest` (moka)
  middleware. Both resolvers wrap their `reqwest::Client` with it in `new()`.
- `src/{client,server}/error.rs` — hand-rolled error enums (this crate predates
  and does not use `snafu`; match the existing style when editing here).

## Gotchas

- **Test-only signature change**: under `#[cfg(test)]`, `server::Resolver::resolve`
  takes an extra `port: Option<u16>` argument and both resolvers hit `http://`
  instead of `https://` (so `wiremock` on localhost works). Non-test callers use
  the plain signature. Don't "fix" the seemingly-inconsistent signature.
- Tests use `reqwest`'s `.resolve(host, addr)` to point fake hostnames at a
  local `wiremock` server rather than mocking the client.
- `clippy::unwrap_used` and `clippy::expect_used` are denied crate-wide
  (`lib.rs` + `lints.toml`); `expect` is only allowed at sites annotated with
  `#[allow(clippy::expect_used)]` where an invariant guarantees success.
- `missing_docs` is warned — public items need doc comments.
- TLS backend is a choice between `rustls` (default) and `native-tls` (opt-in)
  features; they toggle the TLS stack for `reqwest` only — DNS resolution
  (`hickory-resolver`) is TLS-backend-independent.
- Edition is 2024 with `resolver = "3"` (previously 2018, which mismatched
  `rustfmt.toml`'s 2021 formatting target). `rustfmt.toml` also uses
  nightly-only options (`group_imports` / `imports_granularity =
  "StdExternalCrate"`) that stable rustfmt silently ignores — the devshell
  provides a nightly rustfmt ahead of the stable one on `PATH` so `cargo fmt`
  actually applies them.

## CI & conventions

- CI delegates to `famedly/backend-build-workflows` (pinned by SHA in
  `.github/workflows/rust-workflow.yml`) and runs the nextest `ci` profile.
- Clippy lint set is defined in `lints.toml` (consumed by the famedly workflow /
  pre-commit clippy hook), not in-source — add lints there.
- `pre-commit` runs fmt, clippy, and `typos`. Install with `pre-commit install`.
- License: AGPL-3.0-only.

[client-server well-known URI]: https://spec.matrix.org/latest/client-server-api/#well-known-uri
[server-server resolution]: https://spec.matrix.org/latest/server-server-api/#resolving-server-names
