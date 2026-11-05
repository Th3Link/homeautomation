"""The light switch's sensor extension: what it measures goes out on CAN."""

import pytest


@pytest.mark.parametrize("celsius", [18.0, 27.25, -3.5])
def test_bme680_temperature(sim, taster, celsius):
    taster.bme680.set(temperature=celsius)
    sim.expect_can("TemperatureSensor", chip=taster, sensor="bme680@77", value=celsius)


def test_bme680_humidity_and_pressure(sim, taster):
    taster.bme680.set(humidity=63, pressure=987.5)
    sim.expect_can("HumiditySensor", chip=taster, value=63)
    sim.expect_can("PressureSensor", chip=taster, value=987.5, tolerance=0.2)


def test_bme680_gas_resistance(sim, taster):
    taster.bme680.set(gas=250)
    sim.expect_can("GasResistance", chip=taster, value=250, tolerance=1)


def test_scd30_co2(sim, taster):
    taster.scd30.set(co2=1450)
    sim.expect_can("Co2Sensor", chip=taster, sensor="scd30@61", value=1450)


@pytest.mark.parametrize("lux", [3, 850])
def test_veml7700_light(sim, taster, lux):
    taster.veml7700.set(lux=lux)
    sim.expect_can("AmbientLightSensor", chip=taster, value=lux, tolerance=max(1, lux * 0.02),
                   timeout=15)


def test_raw_i2c_reads_the_parts(taster):
    assert taster.i2c(0x77, b"\xd0", 1) == b"\x61"  # BME680 chip id
    firmware = taster.i2c(0x61, b"\xd1\x00", 3)  # SCD30: version word + CRC
    assert firmware[:2] == b"\x03\x42"


def test_values_out_of_range_are_refused(taster):
    from simlib import SimError
    with pytest.raises(SimError, match="humidity must be"):
        taster.bme680.set(humidity=120)
