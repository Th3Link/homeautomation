# cancomponents — component docs (placeholder)

`cancomponents-core`/`cancomponents-hardware` don't have written
requirements/protocol/ADR docs yet — this is a placeholder for that future
work. In the meantime:

- The CAN wire format they share with the gateway is documented at the
  workspace level: [`docs/protocol-can.md`](../../../docs/protocol-can.md).
- The shared code extraction that unified their wire-format types and
  hardware-support helpers with `gateway-core`/`gateway-hardware` is
  documented in [ADR 0012](../../../docs/adr/0012-shared-common-crates.md).
