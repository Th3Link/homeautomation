# ADR 0024: Air quality from the BME680 with Bosch's BSEC 2

**Status:** Accepted

## Context

The light switch's sensor board has a BME680: temperature, humidity,
pressure and a metal-oxide gas sensor. The gas sensor's resistance on its
own says little. It drifts, depends on humidity and temperature, and has
no fixed zero. Bosch's BSEC library turns it into an indoor air quality
index (IAQ, 0–500), a CO₂ equivalent and a breath VOC equivalent (ppm),
and calibrates itself against the air it sees over days.

The protocol already has the messages (`Co2Equivalent` 152, `VocBreath`
153, `AirQuality` 154, fixed point like the other sensors), and the
gateway publishes them. The legacy firmware carried BSEC 2 but never
called it.

BSEC is closed source: a static library per CPU, under Bosch's licence.
There is no open replacement with comparable results.

What the BME680 can't do: measure particulate matter (that takes an optical
sensor such as an SPS30), or CO₂ itself (that takes an NDIR or photoacoustic
sensor such as an SCD41). The CO₂ equivalent is an estimate from the VOCs
people breathe out. It follows a room's occupancy well, but it isn't a CO₂
reading.

## Decision

**BSEC 2.4.0.0 in the node firmware**, linked statically
(`cancomponents-hardware/bsec2`), with the library's default configuration:

- **Low power mode**: BSEC asks for a forced measurement every 3 s, the
  heater at 320 °C for 197 ms. The firmware does what `bsec_sensor_control`
  says. The `bosch-bme680` driver measures, and the heater's set point is
  written into its registers before each measurement; the driver itself
  only knows a fixed 300 °C.
- **Outputs**: IAQ, CO₂ equivalent and breath VOC equivalent go out with the
  temperature, humidity and pressure, once per send interval, under the
  BME680's `sub_id`. An output goes out only once BSEC rates its accuracy at
  1 or more. Before that, during the run-in, it reports fixed start values.
- **Calibration kept**: BSEC's state (221 bytes) is saved to flash every
  4 hours and when the IAQ first reaches accuracy 3, and is loaded at start.
  Without it a restart costs days of calibration.
- **Fallback**: if BSEC doesn't start, the BME680 measures as before,
  without the gas sensor.
- The gateway shows CO₂ and VOC in ppm. In Home Assistant they are
  `carbon_dioxide`, `volatile_organic_compounds_parts` and `aqi`.

## Consequences

- A binary from Bosch in the firmware. It is used only with the BME680, as
  its licence requires.
- The heater runs 6.6 % of the time and adds to the board's self-heating.
  Temperature and humidity are still the BME680's raw readings, not BSEC's
  compensated ones.
- The fixed-point format caps values at 4,095.9: a CO₂ equivalent above
  that is sent as 4,095.9.
- BSEC's configurations for a 3.3 V supply are not loaded. The default
  configuration is for 1.8 V; it differs in the self-heating compensation,
  whose outputs aren't used.
