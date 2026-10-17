# Changelog

## 0.1.1 — 2026-10-18

- Public repository with MIT License and inherited license metadata for every workspace package.
- Expanded README, Rust onboarding, headless JSON command guide and contributor instructions.
- Read-only GitHub Actions token permissions and a bounded CI timeout.
- No changes to Run/Transcript/Composer or SessionEndpoint behavior from the 0.1.0 milestone.

## 0.1.0 — 2026-10-18

Annotated headless milestone including merged PRs #1 and #2:

- Independent Native (Kernel 0.9.0/v4), Pi 1.3.0 and DSH 0.2.1-rc.2 adapters; DSH 0.2.0-rc.2 compatibility.
- Session pointers, native ancestry / frozen-cut history, reconnect, actionables, attachments and version-checked files.
- Memory projection and explicit optional/host capability boundaries, without a disk transcript Mirror.
- Renderer-free Run, Transcript, Composer and connection state, destination-owned drafts and receipt-based optimistic input reconciliation.
- 59 local tests and synthetic-provider integration evidence, including the three presentation paths.

No UI/TekesUI integration, durable client cache, production launcher or large-history performance acceptance is included. Earlier CI attempts could not start because of account billing/spending limits; see individual workflow results for subsequent Linux validation.
