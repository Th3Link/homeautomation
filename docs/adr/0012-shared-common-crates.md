# ADR 0012: Extract `common-core`/`common-hardware`, superseding independent reimplementation

**Status:** Accepted

## Context

`gateway-rs` and `cancomponent-rs` started as separate repositories, each
with its own Cargo workspace. [ADR 0005](../../software/gateway-core/docs/adr/0005-reimplement-wire-protocol-independently.md)
(in `gateway-core`) explicitly chose to reimplement the CAN wire format
from scratch rather than depend on `cancomponent-rs`'s `cc-core`, reasoning
that the duplication was small (a handful of enums, ~30 lines of bit
arithmetic) and that coupling the gateway's build to a separate
repository's availability wasn't worth it for that little code.

That calculus changes once both projects live in one repository and one
Cargo workspace (this reintegration — see the top-level `docs/index.md`).
A path dependency within the same workspace has none of the
separate-repository availability/release-cadence concerns ADR 0005 was
actually worried about, while the duplication itself turned out to be less
trivial than assumed: `gateway-core` and `cancomponents-core`'s
independently-written `can_id.rs`/`can_message_type.rs`/`device_type.rs`/
`device_message.rs` already differ in supporting functionality (`gateway-core`
has `CanId::device_key`/`DeviceType::from_name`/`encode_id_type`;
`cancomponents-core` has `filter_code_mask` for hardware CAN filtering) even
though the wire-format numeric contract itself (discriminants, bit
layout) is identical — exactly the kind of accidental drift ADR 0005
flagged as a real risk of keeping the two independent.

Separately, `gateway-hardware` and `cancomponents-hardware` also duplicate
non-protocol boilerplate that has nothing to do with the wire format:
`SharedFlash` (exclusive access to the on-chip flash), the `console_log!`
muting macro, and the `sequential-storage`-backed config-singleton
plumbing (`Mutex<Option<Config>>` + `init`/`get`, generic over each
crate's own `Key` enum and buffer size).

## Decision

Extract two new workspace crates:

- **`common-core`** (`#![no_std]`, hardware-independent): `can_id`,
  `can_message_type`, `device_type`, `device_message`, and `DecodeError` —
  merged from both `gateway-core` and `cancomponents-core`'s versions as a
  superset (keeping `device_key`, `filter_code_mask`, `from_name`/`name`,
  and `encode_id_type` all in one place). `gateway-core` and
  `cancomponents-core` now depend on it and re-export its modules at their
  same historical paths (`crate::can_id`, etc.), so nothing outside those
  two crates needed to change.
- **`common-hardware`** (esp-hal dependent): `flash::SharedFlash`,
  `logging`'s `console_log!`/`CLI_ACTIVE`, and a generic
  `config_store::{ConfigStore, ConfigCell}` covering the
  singleton-plus-typed-accessors boilerplate. The actual `Key` enums and
  any whole-struct load/default logic (`gateway-hardware`'s
  `LoadOrInit::load_or_init`) stay in each firmware crate, since those
  genuinely differ.

This supersedes ADR 0005: the gateway's wire-format types are no longer an
independent reimplementation — they're the same code as
`cancomponents-core`'s, in `common-core`.

## Consequences

- One compiler-enforced source of truth for the wire format both
  components implement — the drift ADR 0005 accepted as a risk can no
  longer happen silently.
- `common-core`/`common-hardware` become genuine shared dependencies: a
  breaking change to either now requires updating (and re-testing) every
  crate that depends on it, in the same PR, in the same repository. This
  is the coordinated-update cost ADR 0005 wanted to avoid — accepted now
  because the workspace merge removes the separate-repository friction
  that made it costly before.
- Anything that differs between the gateway and node roles (`error.rs`,
  `relais.rs`, the whole-config load/default logic) deliberately stays
  out of the shared crates rather than being forced into a common shape —
  see `common-core`'s and `common-hardware`'s own crate docs.
