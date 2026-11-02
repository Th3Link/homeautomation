# ADR 0024: Air quality from the BME680 with our own open calculation

**Status:** Accepted

## Context

The light switch's sensor board has a BME680: temperature, humidity,
pressure and a metal-oxide gas sensor. The gas sensor's resistance on its
own says little. It drifts, falls with humidity, and has no fixed zero. It
needs a calculation that compensates humidity and calibrates itself against
the air it sees.

The protocol already has the messages: `Co2Equivalent` 152, `VocBreath` 153
and `AirQuality` 154, fixed point like the other sensors. The gateway
publishes them (`co2`, `voc`, `air_quality`) and announces them to Home
Assistant.

Bosch's BSEC library does this calculation. It is closed source, a static
library per CPU under Bosch's licence, and we can't use or redistribute it in
this open repository. A first integration of BSEC 2 was dropped for that
reason.

## Decision

**Our own calculation, as a separate open crate:
[`bme68x-iaq-co2`](https://github.com/Th3Link/bme68x-iaq-co2)** (MIT OR
Apache-2.0, `no_std`, no allocation). It is a Rust rewrite with tests of
[SE_BME680](https://github.com/steveeidemiller/SE_BME680)'s algorithm (MIT),
which is based on raspi-bme680-iaq (MIT), and adds the index and the
equivalents. The crate doesn't talk to the sensor.

- **Driver: [`bme68x`](https://crates.io/crates/bme68x)** (BSD-3-Clause).
  The previous `bosch-bme680` only knew a fixed heater at 300 °C.
- **Measuring** (`cancomponents-hardware/src/environment.rs`): every 3 s
  the sensor measures once in forced mode, with the heater at 320 °C for
  150 ms. The cycle stays constant, because the gas resistance depends on
  how long the heater rested. The heater's set point is recomputed when the
  sensor's temperature moves by 3 K.
- **The calculation:**
  - The gas resistance is multiplied by `exp(0.03 × absolute humidity)`.
  - **Warm-up:** at least 30 s, until the resistance stops falling.
  - **Burn-in:** at least 5 minutes, until 100 values are collected. The
    mean of the highest compensated resistances is the gas ceiling ("clean
    air").
  - **Operation:** new highs go into the ceiling. After 30 minutes without
    one, a value is rotated out, so the ceiling follows drift.
  - **Air quality** = `(resistance / ceiling)² × 100 %`.
  - **Accuracy** 0–4 from how close together the ceiling's values are and
    how long it ran.
- **Index and equivalents**, from a documented model:
  - A metal-oxide sensor follows `R/R0 = (C/C0)^−β`, so the air quality gives
    the VOC concentration relative to clean air.
  - The **index** (0–500, 0 = clean) is linear in the logarithm of that
    concentration: 25 for clean air, 250 for ten times the VOCs.
  - The **breath-VOC equivalent** is 0.5 ppm × the concentration.
  - The **CO2 equivalent** is 10 ppm × the index, at least 400 ppm.
  - **Phase 1** (now): the constants come from published anchor points only
    (Bosch's IAQ scale, BSEC's clean-air values, "CO2 equivalent ≈ 10 × static
    IAQ").
  - **Phase 2:** a Sensirion SCD30 (real CO2, NDIR) is wired to the same
    sensor board, and the constants are fitted against it.
- **Sending:** once per send interval go out temperature, humidity and
  pressure. Once the burn-in is over (at least 5 minutes and 100 values),
  `AirQuality` (the index), `Co2Equivalent` and `VocBreath` (ppm) go out
  too. During the burn-in the sensor's resistance is still rising, so every
  reading would look like clean air.
- **Calibration** (422 bytes, CRC-protected):
  - It goes into the config store (`AirQualityState`) every 4 hours and when
    the accuracy first reaches "high". The store's buffer grew from 256 to
    512 bytes for it.
  - At start it is restored. The sensor warms up, then carries on with the
    restored ceiling instead of a new burn-in.
- **Without gas:**
  - If the heater can't be set up, the sensor measures temperature, humidity
    and pressure as before.
  - A gas sensor without a heat-stable reading for 5 minutes is logged.

## Consequences

- No closed code in the firmware. The calculation is ours to test, explain
  and change.
- The CO2 equivalent is an **estimate from VOCs**, not a CO2 measurement.
  Cleaning agents, cooking or paint raise it too, and CO2 from other sources
  doesn't show. Until phase 2 its constants are anchor points, not a fit.
  Real CO2 needs an NDIR sensor (SCD30, SCD41).
- The air quality is relative to the cleanest air this sensor saw. The
  calibration needs hours to days in the same place.
- The heater heats the sensor board a little more (about 5 % duty cycle).
- The BME680 on the bench light switch fails Bosch's self-test
  (`GasResponseTooSmall`): its gas reading never changes and is never
  heat-stable. It sends no air quality, as intended. Testing the calculation
  needs a working sensor.
- The fixed-point frame caps values at 4095.9. A higher CO2 equivalent
  arrives as 4095.9. That only happens with an index above 409.
