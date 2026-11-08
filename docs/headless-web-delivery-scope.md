# Headless and Web delivery scope

This delivery serves the existing TekesWebUI over its authenticated local host. The shared product behavior stays in AgentKit; the browser host owns HTTP authentication, process lifetime, credentials, configuration and serving static assets.

## Source closure

The delivery branch is based on AgentKit main `1dcc52153ea45ba6ce73a135a8d8fe8d3406360c` and brings the complete Rust crate workspace from `2c8a2fb83c8df1834b087d23970b37a4b7f0d109`, including `contract`, `session`, `presentation`, all three `endpoint-*` crates, `web`, and `headless`, plus their exact root `Cargo.lock` and generated public TypeScript contract. The latter currently has no `index.js`; the package exports the declarations used by TekesWebUI.

Pi's independent overlapping read and pending-request cleanup change from `3f5b2e5f4449297d6833f43fe366f5ea4e741d24` is included while retaining the candidate's public reading identity projection. `get_entries` and `get_state` overlap, both must succeed before a frame is emitted, and request guards forget pending local reads after success, error, timeout or cancellation. This is not a claim that the existing Pi relative performance failures have passed.

No new `ui/` package, TekesUI dependency, native rendering crate, or macOS UI-consumer job is part of this source closure. Existing mainline experimental UI files and their static ownership guard are retained unchanged. Native-only UI gates remain attached to the native UI delivery. Shared Rust workspace checks, public contract checks, SEP protocol acceptance, performance requirements and existing WebUI browser acceptance still apply to this delivery.

## Checks retained in CI

- Rust format, workspace Clippy and all workspace tests with the checked-in lockfile.
- Opt-in acceptance-source-witness Clippy and tests for presentation and headless service.
- Pi extension JavaScript tests, the headless dependency boundary check and the existing mainline static UI ownership guard.

CI does not declare real Harness protocol acceptance, browser behavior, model use or performance accepted. Those require separate evidence against the exact delivered binaries and runtimes.

## Consumer compatibility and host lifecycle

The existing daily entry uses WebUI `0.1.23` assets and an AgentKit `0.3.8` service. The isolated WebUI `0.2.0` integration is based on public main `03b1966b5ac321534798ef64621c5eeaab24448a` and consumes an immutable public AgentKit candidate contract. The older primary checkout `0.1.19` is preserved WIP, not the installed version. A contract pin does not identify the installed `tekes-agent-web` binary: the private host configuration controls that path independently. Before a release pin is changed, validate the existing host's HTTP and NDJSON envelope against the delivered JSON-lines service and publish an immutable contract commit.

The private JSON-lines service owns normalized session state and delegates all Harness behavior to Native, Pi and DSH public adapters. The WebUI local host translates its HTTP requests and observations to that service. Closing a browser releases observation, not the service or a Harness task. After a host restart, session discovery and history depend on each Harness's public recovery contract; this is not an assertion that queued or in-flight work survives every restart.

## Remaining release evidence

The branch is not delivered until the host build uses the exact AgentKit version, an independent public consumer compiles against its immutable contract pin, each Harness's supported protocol paths pass the finite #55/#56 matrix, applicable performance results meet their original budgets, and the installed WebUI passes browser acceptance. One existing configured real-model run is required. Do not reinterpret a supported failure as `Unsupported`. The existing WebUI setup/configuration entry point is reused; only a demonstrated compatibility or daily-use defect should change it.
