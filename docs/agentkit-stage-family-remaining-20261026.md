# #56 remaining family acceptance: bounded offscreen scope

This is a source/evidence audit, not new protocol acceptance. Scope base5b,
blocking DSH candidate4274, ClientKit c289 and public TekesUI50ec49f remain frozen.
The coordinator's new independent read-only reviewer found no need for new product
features or dependencies to exercise the existing child-pane host.

| Capability | Existing evidence | Required next slice |
| --- | --- | --- |
| Pi/DSH original FamilyCatalog opens the SidePanel child pane | `rust-agent-ui-live-child-pane-source-results-20261024.json`, source04481a7, original programs only | Repeat the applicable original host proof with immutable current product inputs and explicit new program/runtime hashes; do not relabel old receipts. |
| Nonempty parent draft survives child activity; retired child callback loses authority | Same old real host proof plus component/source retirement regressions | Bind current host to original parent, retain draft before/after, close original pane and reject the old source witness. |
| Child pane history paging and reconnect | Old host receipt explicitly not_run; current Pi/DSH primary-child receipts prove a different consumer | Exercise the mounted pane's actual read/refresh intent, original child and parent authority, actual page/cursor, and explicit reconnect. Confirm source/epoch and retirement fences; primary-child Run is not a substitute. |
| Host and independent consumer | Existing original demo composes SidePanel, FamilyCatalog and child Transcript; CI runs a separate public API consumer | Keep real-harness host proof separate from standalone consumer/API build and strict reading. Neither waives the other. |

The reusable runner is `scripts/test-real-child-pane-source.py`. It explicitly
launches `--components --transcript-content --offscreen`, creates no native
window, and writes its own rendered PNG as acceptance evidence. This is not OS
screen capture or desktop/IME/AX acceptance. No runner was launched by this audit.
`ui/demo/src/child_pane_host.rs::open_child_for_parent` captures and rechecks the
original parent around the awaited read. Native's separate bounded pane proof
is a useful API example, not evidence that Pi/DSH have passed the same path.

Before any next run, verify the runner's remaining controls/observer support,
freeze only acceptance-tool changes and their exact product source closure,
check live tasks and runtime identity, then let the main coordinator serialize
Cargo on the existing SSD shared target. Current primary WIP is excluded.
Archive old programs as evidence and execute only explicit fixed-role programs.
No new application identity, native window, TCC request, SDK or Kernel edit.

Remaining #55 tool/reasoning/usage, Native queue Kernel86, strict TekesUI53 and
preregistered non-desktop performance retain their own status. Do not expand
#56 to future #60–#66 features or declare the phase delivered from this audit.
