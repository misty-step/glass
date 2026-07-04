# Glass Repo Instructions

Glass is the native Rust stage for live agent surfaces. It adopts the Sideshow
grammar and owns the implementation.

## Contracts

- Rust only unless a platform boundary is named in the change.
- The frozen surface kinds are `html`, `diff`, `image`, `trace`, `markdown`,
  `terminal`, `mermaid`, `json`, and `code`.
- User feedback delivery is server-owned: one `agent_seq` cursor per session,
  shared by wait/drain and publish/update piggyback responses.
- Agent-authored or rich rendered surfaces must be served by a real URL with a
  `Content-Security-Policy` response header that includes `sandbox` and omits
  `allow-same-origin`.
- Asset ids are the lowercase SHA-256 of the uploaded bytes.
- Bridge feed display is retired for Glass-wired lanes; Bridge may still relay
  events, but Glass owns human-facing live surfaces.

## Gate

Run the repo gate before completion:

```sh
./scripts/check.sh
```

The gate is:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Do not weaken the gate to ship.
