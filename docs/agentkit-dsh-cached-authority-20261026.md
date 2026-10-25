# DSH cached inventory projection boundary — 2026-10-26

Issue #67 narrows the DSH adapter behavior to the public 0.2.1-rc.2 session
contract. The frozen declaration used for this check is
`@deepseek-ai/dsh-api-session-controller` 0.2.1-rc.2,
`lib/types/types.d.ts` SHA-256
`81017e58e95b3a58d62e0c3d540bd6286b645686ed3d208acac1dd2a206bea27`.
`SessionProjectionHints` at lines 52–65 defines `kind: cached | sequenced`,
where `cached.asOfSeq` belongs to the persisted record's sequence space and
cannot be compared with a connected session. `SessionProjectionsValue` at
lines 414–423 permits only sequenced, migration-required, or null for the
explicit read. Follow baselines at lines 538–546 have their own sequenced
baseline shape without a `kind` discriminator.

The comparison point is TekesClientKit c289
(`c289bc9d33edcd83a782ac17927a8c65a027eccd`), recorded in
`docs/source-baseline.json`. Its `ProjectionBlock` contract is sequence plus
values and has no cached authority variant. AgentKit therefore keeps cached
DSH list hints outside that sequenced model.

## Adapter behavior

- Only `session/list` inventory snapshots may carry `kind: cached`. The
  adapter validates its integer `asOfSeq` (minimum -1), object `values`, and
  optional string-or-null `title`, then preserves the original block under
  the reserved `dshCachedProjectionHints` summary field.
- The cached block is removed from `projections`, so cached inbox and
  working-directory values cannot enter queue reconciliation, projection
  ordering, or current directory state. A cached string title fills only an
  absent top-level summary title. Existing top-level values, including null,
  are retained; a live sequenced title projection remains in its original
  location and keeps precedence in sidebar title resolution.
- Connected `api-session/added`, control baselines/deltas, explicit
  `session/projections`, follow, and history paths do not accept cached as
  sequenced state. Unknown kinds, malformed cached headers, and remote use of
  AgentKit-reserved keys remain protocol errors. `migration-required` remains
  non-authoritative.

## Evidence boundary

This adapter change and its focused regression cases do not prove the original
real DSH reconnect. The 2026-10-26 failure reported
`unknown DSH projection authority`, but its exact triggering wire block was
not captured; the cached contract gap is independently established by the
fixed public declaration. Parent coordination must rerun the affected DSH
family reconnect against the frozen source and runtime, preserve the original
failure receipt, and record the exact candidate/program/runtime identity.
No Cargo, runtime, desktop, or protocol validation was run for this change.
