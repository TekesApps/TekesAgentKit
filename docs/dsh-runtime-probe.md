# AgentKit DSH runtime identity probe

`integrations/dsh-runtime-probe` is the DSH Host plugin that provides AgentKit's authenticated runtime identity endpoint. DSH 0.2.1-rc.2 exposes `getDshRuntimeVersion()` only inside its local `@deepseek-ai/dsh-app-boot` package; its Remote handshake does not include a version. The plugin resolves the official package from the active DSH CLI entry (`process.argv[1]`) and returns that function's version from `GET /api/agentkit/harness-version`. It does not read or modify provider settings, credentials, session data, or workspace files. An unresolved runtime returns HTTP 503; it never guesses.

The endpoint requires `ctx.connection.admit(req)` to accept the request and returns no version to unauthenticated callers. Only the DSH 0.2.1-rc.2 runtime is pinned as a peer dependency for the first delivery. The endpoint is exact and intentionally separate from the Tekes workspace plugin route. Install one provider for this route in the active Host profile; do not install competing providers that register the same exact path.

## Install into the existing Host

Use the already installed DSH CLI and Node runtime. From the AgentKit checkout, install this local plugin into the same `web` profile used by the existing WebUI connection:

```sh
pnpm dsh plugin --profile web add /absolute/path/to/TekesAgentKit/integrations/dsh-runtime-probe
```

Restart the existing Host through its normal DSH profile manager so the plugin loads. This does not install a second DSH runtime and does not migrate or rewrite user data. Verify the route through the authenticated WebUI/AgentKit connection; a direct unauthenticated request is expected to receive 401.

The existing workspace plugin remains responsible for its workspace endpoints. If another plugin already registers `/api/agentkit/harness-version`, remove that duplicate provider from the selected profile before enabling this one.

## Focused checks

`node --test integrations/dsh-runtime-probe/test/*.test.mjs` covers authentication, method rejection, exact version source, malformed values, and fail-closed behavior. These checks are not a live DSH Host acceptance. The AgentKit DSH endpoint now reads `/api/agentkit/harness-version`; the consumer and plugin must be deployed together. An authenticated real-host check remains a separate delivery gate.
