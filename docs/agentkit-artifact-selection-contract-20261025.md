# AgentKit exact artifact selection contract

Status: Issue #36 implementation WIP. The reference now appears as an optional
typed annotation field, preserves legacy payloads, and is returned through the
Run host adapter for its document reveal callback. Compilation/consumer/runtime
verification is still pending.

## Purpose and boundary

This contract preserves the exact source selection attached to an annotation.
It complements `ComposerAnnotation.source_locator`: that locator identifies a
transcript item, while this reference identifies a document artifact, its type,
and a stable field/block path inside it. Display row numbers are never identity.

The host supplies immutable public snapshot identity and the currently resolved
field text. AgentKit performs value validation and deterministic resolution
only. It does not read a database, file, URL, or private host store, and it does
not fetch or mutate artifacts. A host mutation must independently enforce the
captured revision/digest at its write boundary.

## Capture value

`ArtifactSelectionReference` contains document ID, artifact ID and type, a
stable sequence of field/block path components, revision, digest, a UTF-16
location and length, the selected quote, and surrounding context. Identity,
revision, and digest components must be non-empty. The digest is opaque and
host-defined (for example, a namespaced SHA-256 value); producer and resolver
must use the same convention. No digest algorithm or file format is imposed.

Capture accepts a host field string and a non-negative UTF-16 range, including
a zero-length caret range as ClientKit permits. The start and
end must be representable Unicode scalar boundaries in that string; a range
that splits a surrogate pair is invalid. The quote is taken from exactly that
range. Before/after context is captured up to the requested UTF-16 unit limits,
clamped to scalar boundaries without crossing the selection. This permits
Chinese and supplementary characters such as emoji without confusing UTF-8 byte
offsets, Unicode scalar counts, or UTF-16 offsets. Context is provenance only;
it is never used to relocate a stale selection.

Persisted offsets use the existing ClientKit/AgentKit JSON integer convention:
integral spellings such as `1`, `1.0`, and `1e0` decode identically; fractional,
negative, boolean and string values are rejected. The reference's explicit
validation also enforces the shared JSON-safe range limit and checks addition
before text slicing.

## Resolution

Resolution receives the captured reference, an optional public target
identity/type/path/revision/digest, and optional current field text. It returns
one of:

- `Exact`: all identity fields match, revision and digest match, the UTF-16
  range is valid in current text, and the text selected at that exact range is
  byte-for-byte equal as a Rust string to the captured quote.
- `Stale`: the same target is present, but revision or digest changed, or its
  range/quote no longer validates. This includes invalid or out-of-bounds
  current text ranges.
- `Unavailable`: target identity/type/path or current field text is absent, or
  document/artifact/type/field path points to a different target. There is no
  best-effort lookup by quote, context, or similar text.

Thus the same text in a different artifact, document, type, or field/block is
unavailable. A changed source is stale even when the quote still appears
elsewhere. Context never upgrades stale/unavailable to exact. The original
reference remains unchanged on every result.

## Existing annotation integration

`ComposerAnnotation.artifact_reference` is optional, with camel-case wire name
`artifactReference`. Old payloads decode with the field absent and serialize
without adding a null field. Existing `source_locator` remains independently
available for transcript provenance. If both are present, their distinct
identities are preserved; they are not coerced into one another. Annotation
validation requires the typed selection quote to equal the annotation excerpt.
Quoted excerpt/comment strings remain JSON data in the existing annotation
compiler envelope and retain the existing untrusted-source instruction.

The existing Composer context `Open` intent is the reveal callback boundary.
`RunBinding.reveal_annotation_reference` accepts only that action on an
annotation, checks the original destination and exact live context baseline,
and returns the immutable typed reference (or `None` for legacy annotations).
The host resolves it against its current public document snapshot, then decides
how to reveal/highlight. AgentKit does not access a document store or retarget a
stale range. This adapter returns data; the host callback remains responsible
for performing navigation.

## Dependencies and verification

The implementation should use only the existing standard library and Serde
already declared by `tekes-agent-contract`; no new digest or Unicode dependency
is required. The reference type is exported through the public Rust contract
and included in the generated TypeScript declaration.

Tests are intended to cover exact identity and revision/digest, wrong and
missing scope/type/field/block, UTF-16 boundaries and overflow, range and quote
staleness, identical text in distinct sources, and Chinese/emoji capture. No
Cargo build, protocol replay, public second-consumer check, desktop, IME, AX, or
host runtime reveal has been run for this WIP.

## ClientKit comparison

The current ClientKit `ComposerArtifactSelectionReference` WIP uses
document/artifact/type/field-path identity, opaque revision/digest, UTF-16
location/length, context, and `Exact`/`Stale`/`Unavailable` behavior. It rejects
wrong identity or absent resolution inputs as unavailable, and revision/digest
or range/quote mismatch as stale; it does not retarget by similar text. This
candidate preserves those semantics and makes capture-time quote/context
extraction and surrogate-boundary handling explicit. ClientKit sources are an
uncommitted candidate, not a released dependency or API pin.
