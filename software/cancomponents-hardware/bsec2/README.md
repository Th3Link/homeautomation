# BSEC 2 (Bosch Sensortec Environmental Cluster)

`libalgobsec.a`: Bosch's BSEC 2.4.0.0, built for the ESP32 (Xtensa,
`elf32-xtensa-le`, January 2023), closed source. It came with the legacy
firmware (`legacy/cancomponents/components/BSEC2`, unused there) and is the
same file. `build.rs` links it statically; `src/bsec.rs` declares the
functions it uses (from Bosch's `bsec_interface.h` and `bsec_datatypes.h`,
see the legacy component).

It runs with its built-in default configuration. Bosch ships configurations
per supply voltage and calibration window (for example
`bme680_iaq_33v_3s_4d`); one has to match the library's version exactly.

Licensed by Bosch Sensortec under its BSEC licence terms, for use with Bosch
sensors only; it is not covered by this repository's GPL.

What it computes from the BME680, and how the firmware drives it:
ADR 0024 (`gateway-core/docs/adr/0024-bsec-air-quality.md`).
