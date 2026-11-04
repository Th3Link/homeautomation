"""DS18B20s on the relay board's 1-Wire header (modelled in QEMU)."""

# The sensors' serials as the node sends them (ROM bytes 1-6, see
# `ds18b20` in tests/sim.toml: the chip's index and the sensor's number).
FIRST, SECOND = "010003000000", "020003000000"


def test_both_sensors_report(sim, relais):
    relais.ds18b20_0.set(temperature=21.0)
    relais.ds18b20_1.set(temperature=45.0)
    sim.expect_can("TemperatureSensor", chip=relais, sensor=FIRST, value=21.0, timeout=40)
    sim.expect_can("TemperatureSensor", chip=relais, sensor=SECOND, value=45.0, timeout=40)


def test_below_zero(sim, relais):
    relais.ds18b20_0.set(temperature=-7.5)
    sim.expect_can("TemperatureSensor", chip=relais, sensor=FIRST, value=-7.5, timeout=40)


def test_a_sensor_pulled_off_is_reported_lost_and_back(sim, relais):
    relais.ds18b20_1.set(plugged=0)
    sim.expect("LOST", chip=relais, kind="serial", timeout=90)
    sim.expect_can("DeviceError", chip=relais, timeout=10)
    relais.ds18b20_1.set(plugged=1, temperature=46.5)
    sim.expect("recovered", chip=relais, kind="serial", timeout=120)
    sim.expect_can("TemperatureSensor", chip=relais, sensor=SECOND, value=46.5, timeout=60)
