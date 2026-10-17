# Contributing

Issues and pull requests are welcome. Please describe the concrete session behavior, affected harness/version and a small reproduction. Remove credentials and private session content from logs and fixtures before posting them publicly.

## Development

Use the toolchain pinned in `rust-toolchain.toml` and preserve `Cargo.lock`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-headless-boundaries.py
```

Public CI runs these checks on Linux. Real-harness checks are optional local integration tests requiring separately installed compatible runtimes; see the README. Do not commit binaries, personal runtime state, credential files or private conversation logs. Synthetic providers and minimized fixtures are preferred.

## Architecture rules

- Keep this repository Rust-native. Do not add Swift sources or require a Swift toolchain.
- Keep contract/session/adapters independent of presentation and UI; presentation can consume their public API. Components are not yet implemented.
- Treat Kernel, ClientKit and harness sources as external references. Do not introduce implicit sibling path dependencies or direct private database reads.
- Scope all identities to an endpoint and session. Pi opaque entry IDs are not journal sequence numbers.
- Preserve baseline/generation validation, frozen pagination cuts, native ancestry and authoritative/transient separation.
- Do not retry uncertain mutations automatically. Preserve captured submission destinations, draft ownership, receipt identity and unknown-outcome reconciliation.
- Expose product authentication/installation/settings through explicit host inputs and callbacks.
- Change capability declarations only with implementation and meaningful behavior evidence. Unsupported should be explicit.

Current source provenance and known limits are documented under `docs/`. Update the relevant capability/behavior matrix with a protocol or presentation change. Keep local/static tests, actual harness checks, Linux CI, UI and performance acceptance distinct.

## Versions and licensing

This is an early `0.1.x` library, not yet published to crates.io. Public APIs can evolve; document material changes in `CHANGELOG.md`. Milestone tags are annotated and never moved. License and public distribution documentation first appear in `v0.1.1`; `v0.1.0` remains the earlier immutable headless milestone.

Contributions are provided under the repository's [MIT License](LICENSE). Third-party code must retain its required attribution and compatible license terms.
