# Issue #25 Summary follow-up (2026-10-24)

This follow-up records the remaining Summary contract review against the current
read-only ClientKit sources. Scope is the Rust headless projection, component
publication contract, and existing source-owned callbacks. It does not claim
desktop, IME, native harness, or full ClientKit parity.

## Fixed: loaded Summary without usage data

`SummaryProviders.usage` emits six `Unavailable` rows whenever the current
thread mirror is loaded, including when `usageRows` is empty
(`TekesClientKit/Sources/Composer/Agent/SummaryProviders.swift`,
`SummaryPanelView.swift`; SHA-256 `1f2eb4048c30bfde58edd72bd2e50b8e22d211272c609c9d2108c53b6bddf663`,
`72049a4d18530e799eeea3556eebd191c51383ad3c0d31ce1308d5daaf7c8246`). Rust previously omitted the whole usage section when
`SummaryFacts.usage` was `None`. `SummaryFacts::sections` now retains the six
rows with an explicit unavailable marker, and the existing usage formatter
renders that marker as `不可用`. A missing usage projection remains distinct
from known zero counts, a zero-denominator hit rate, and incomplete/unknown
cost.

The total remains the saturating sum of the four authoritative counters, as in
ClientKit's current SQLite projection decoder (`ClientSQLiteSessionStore.swift`,
SHA-256 `547e4359102a638c1c791275149f1d6d5b5f5ebe7897b8b5453895a536f44cc6`). The SessionEndpoint projection
does not currently expose an authoritative cost; Rust continues to leave it
unknown rather than estimate it.

## Static review: actions, ownership, slots

- Default task/subagent/artifact actions are typed `SummaryIntent`s. Publication
  validates action scope before replacing the current model. The host still
  decides whether to execute navigation or file opening.
- Action bindings are replaced on every successful publication. Row identity
  includes owner and full action identity, and `take_intents` only resolves
  current bindings while the section is expanded. Existing tests cover a
  queued action after the section is collapsed and a queued task action after
  its pointer changes.
- Custom host rows are limited to passive rows in host sections. Their token
  carries Summary instance, owner, revision, disclosure epoch, row identity,
  and content key. Existing tests cover owner/revision changes, collapse ABA,
  replacement, and unmount/remount. No source-owned callback or slot fencing
  defect was found in this static pass.
- The public input deliberately accepts host-projected facts and host sections;
  structural validation cannot prove the host's upstream business authority.
  That remains an explicit host contract, not an endpoint permission.

## Verification boundary

Regression cases were added in `crates/presentation/tests/panels.rs` for the six
rows and `ui/demo/tests/summary_component.rs` for unavailable formatting. This
subtask did not run Cargo, UI tests, a desktop session, or a real harness. The
changed Rust sources may receive `rustfmt` and `git diff --check`; test results
must be reported only by the coordinating session after its shared validation.
