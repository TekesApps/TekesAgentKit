# Standalone public Agent UI consumer

Toolchain: Rust `1.95.0` and Python `3.11+` (`3.12` used for this audit). In the commands below,
`python3` must select that interpreter; the older macOS system Python does not provide `tomllib`.

This fixture is a separate Cargo package that exercises the component crate's public constructors
without importing `tekes-agent-ui-consumer-check`, `RunSurface`, or `ComponentDemo`. It demonstrates
mounting the current experimental `RunPort`, `Sidebar`, and `SidePanel` APIs in a TekesUI
`ContentHost`.

The generated manifest uses an explicit local development path to this repository's
`ui/components` package. That package is `0.0.0` / `publish = false`; the path is a development
override, not a release dependency. Current workspace and dependency evidence is recorded in
[`rust-agent-ui-api-contract-audit-20261024.md`](../../docs/rust-agent-ui-api-contract-audit-20261024.md).
The exact TekesUI input is pinned at `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c` in the UI workspace
manifest and lock. A published AgentKit UI version does not exist yet.

Generate the external package and its receipt without building:

```sh
python3 scripts/check-public-agent-ui-consumer.py
```

The command prints the fingerprinted fixture path. Preserve the seeded UI lock as
`Cargo.lock.seed`, copy it to `Cargo.lock`, and resolve the standalone graph offline. The package has
an independent `[workspace]`; it cannot join AgentKit's outer workspace by directory inheritance.
Run this sequence serially with the shared target directory after checking available
disk/process state:

```sh
CONSUMER_DIR=.runtime-fixtures/issue15-contract-20261024/external-consumer-58521a1-63d36cd67c
cp "$CONSUMER_DIR/Cargo.lock.seed" "$CONSUMER_DIR/Cargo.lock"
cargo generate-lockfile --manifest-path "$CONSUMER_DIR/Cargo.toml" --offline
CARGO_TARGET_DIR=/tmp/agentkit-layout-text-292-target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 \
RUSTUP_TOOLCHAIN=1.95.0 \
cargo run --manifest-path "$CONSUMER_DIR/Cargo.toml" --locked --offline
```

If the preparer prints a different fingerprinted path, use that path for `CONSUMER_DIR`.

The program only constructs each public component and converts it to a `ContentHost`; it does not
claim a rendered/native window, process-container integration, endpoint data flow, IME, or complete
ClientKit View parity. Preserve both `Cargo.lock.seed` and the resolved consumer `Cargo.lock` as
separate dependency receipts.

Recorded execution for this audit: the independent consumer exited 0 with
`cargo run --locked --offline`. Its metadata contained the external root plus AgentKit components,
contract, presentation, and session packages; the six resolved TekesUI crates all used the pinned
revision and their sources matched the 243-input review manifest. Receipts are under `.runtime-fixtures/issue-batch-20261024/public-consumer-*`; the archived
executable is `.runtime-fixtures/issue-batch-20261024/agentkit-public-ui-consumer`. This is compile and
constructor/mount evidence only; it is not a rendered window or ProcessContainer acceptance.
