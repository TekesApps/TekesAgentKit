# Sidebar durable identity tie order

## Contract source

ClientKit's `SessionEndpointStorageIdentity.thread(endpointID:sessionID:)` returns the bare session ID for `legacy-tekes-appserver`. For every other endpoint it composes the UTF-8 byte-length-prefixed endpoint scope and the UTF-8 byte-length-prefixed `session` scope. `ClientSessionSidebar.sessionRows` uses the durable thread ID as the final tie-break when localized title comparison is equal; pinned rows first use their persisted `pinOrder`, then that same durable ID.

AgentKit now exposes one pure `compare_sidebar_identity` helper from presentation. Its storage key is private. Recent-search ranking ties in bindings and component title/pin fallbacks all call this helper. Public `SessionAddress` values, routing, explicit pin positions, and the host-provided localized title comparator are unchanged.

## Evidence and limit

The presentation tests cover endpoint names with different UTF-8 lengths, a multibyte endpoint, the legacy Native identity, and separator characters in both address fields. Component tests cover equal localized titles, equal session IDs on separate endpoints, and the precedence of explicit pin order.

Rust orders these generated keys by their UTF-8 bytes. This matches the durable key's byte representation and the intended SQLite binary tie order. The inspected Swift title path uses `String <`; full equivalence for canonically equivalent or otherwise unusual Unicode strings was not established here, so this change does not claim complete Swift Unicode comparison parity. The main session verified the presentation package and full UI workspace; final validation counts are recorded in the companion active-knownness results receipt. The native session-menu boundary regression now uses this durable order too (`pi` before `native` when explicit pin order is absent).
