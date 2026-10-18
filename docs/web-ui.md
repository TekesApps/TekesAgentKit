# Browser UI ownership

Browser UI has moved out of AgentKit to the standalone MIT React + TypeScript
project [TekesWebUI](https://github.com/TekesApps/TekesWebUI).

AgentKit retains the headless `crates/web` JSON-lines bridge, normalized SessionRuntime
and Harness adapters. Its binary name remains `tekes-agent-web` for compatibility.
Hosts own authentication, listener, credentials, permissions and process lifetime.
Swift reusable Agent views remain in ClientKit.

The shared annotation DTOs and TypeScript bindings are exported by the public
`@tekesagentkit/client-contract` package. `composer.compileAnnotations` compiles input
inside AgentKit; `follow(view)` projects `inputPresentation` for history rendering.
See [the service contract](../crates/web/README.md#annotation-interface).
