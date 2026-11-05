"""Relay boards without a µC on the extension bus: bank 1-4 of a Relais
command, next to the sensors or PWM boards on the same bus."""

import pytest

# DeviceError: Relais, InvalidData, Warning, then bank and channel.
NO_SUCH_RELAY = [6, 1, 1]


def relais(relay, on, bank, ms=0):
    """A Relais command: [relay, state, duration ms (24 bits LE), bank]."""
    return [relay, 3 if on else 0, ms & 0xFF, ms >> 8 & 0xFF, ms >> 16, bank]


def test_boards_found(taster, pwm):
    assert len(taster.bank(1)) == 12
    assert len(taster.bank(2)) == 16
    assert taster.bank(3) is None
    assert len(pwm.bank(4)) == 12
    show = taster.console("show")
    assert "relais_boards: 1=relais 2=ssr" in show


@pytest.mark.parametrize("bank,relay", [(1, 0), (1, 11), (2, 0), (2, 15)])
def test_relay_on_and_off(sim, taster, bank, relay):
    sim.send_can("Relais", to=taster, data=relais(relay, True, bank))
    sim.expect(rf"bank {bank} relays? .*\b{relay}\b.* on", chip=taster, kind="relay", timeout=5)
    sim.expect_can("RelaisState", chip=taster, data=[relay, 3, 0, 0, 0, bank], timeout=5)
    assert taster.bank(bank)[relay]
    assert sum(taster.bank(bank)) == 1
    sim.send_can("Relais", to=taster, data=relais(relay, False, bank))
    sim.expect(rf"relays? .*\b{relay}\b.* off", chip=taster, kind="relay", timeout=5)
    assert not any(taster.bank(bank))


def test_on_for_a_while(sim, taster):
    sim.send_can("Relais", to=taster, data=relais(9, True, 2, ms=1500))
    on = sim.expect(r"bank 2 relay 9 on", chip=taster, kind="relay", timeout=5)
    off = sim.expect(r"relay 9 off", chip=taster, kind="relay", timeout=5)
    assert off["bank"] == 2
    assert 1.2 < off["t"] - on["t"] < 2.5
    # The report after the time runs out names the bank too.
    sim.expect_can("RelaisState", chip=taster, data=[9, 0, 0, 0, 0, 2], since=on["seq"], timeout=5)


def test_same_relay_on_two_banks(sim, taster):
    sim.send_can("Relais", to=taster, data=relais(3, True, 1))
    sim.expect(r"bank 1 relay 3 on", chip=taster, kind="relay", timeout=5)
    sim.send_can("Relais", to=taster, data=relais(3, True, 2))
    sim.expect(r"bank 2 relay 3 on", chip=taster, kind="relay", timeout=5)
    assert taster.bank(1)[3] and taster.bank(2)[3]
    sim.send_can("Relais", to=taster, data=relais(3, False, 1))
    sim.expect(r"bank 1 relay 3 off", chip=taster, kind="relay", timeout=5)
    assert not taster.bank(1)[3] and taster.bank(2)[3]
    sim.send_can("Relais", to=taster, data=relais(3, False, 2))
    sim.expect(r"bank 2 relay 3 off", chip=taster, kind="relay", timeout=5)


@pytest.mark.parametrize("bank,relay", [(3, 0), (1, 12), (0, 0)])
def test_no_such_relay(sim, taster, bank, relay):
    """No board at bank 3, the relay module has 12 relays, and a light
    switch has no board of its own."""
    sim.send_can("Relais", to=taster, data=relais(relay, True, bank))
    sim.expect_can("DeviceError", chip=taster, data=NO_SUCH_RELAY + [bank, relay], timeout=5)
    sim.no_event(kind="relay", chip=taster, within=1)


def test_console(taster):
    assert "relais 2/15 -> On" in taster.console("relais 15 on 2")
    assert taster.bank(2)[15]
    taster.console("relais 15 off --bank 2")
    assert not taster.bank(2)[15]


def test_sensors_keep_working(sim, taster):
    """The sensors share the bus with the relay boards."""
    taster.veml7700.set(lux=777)
    sim.expect_can("AmbientLightSensor", chip=taster, value=777, tolerance=8, timeout=20)


def test_next_to_pwm_boards(sim, pwm):
    sim.send_can("Relais", to=pwm, data=relais(5, True, 4))
    sim.expect(r"bank 4 relay 5 on", chip=pwm, kind="relay", timeout=5)
    sim.send_can("LampGroup", to=pwm, data=[255, 0b1, 0, 0, 1])
    sim.expect(chip=pwm, kind="device", timeout=5,
               where=lambda e: e["device"] == "pwm-1" and e["state"]["duty"][0] == 100)
    sim.send_can("Relais", to=pwm, data=relais(5, False, 4))
    sim.expect(r"relay 5 off", chip=pwm, kind="relay", timeout=5)
    sim.send_can("LampGroup", to=pwm, data=[0, 0b1, 0, 0, 1])


def test_no_relay_on_while_booting(sim, taster):
    sim.send_can("Relais", to=taster, data=relais(0, True, 1))
    sim.expect(r"bank 1 relay 0 on", chip=taster, kind="relay", timeout=5)
    mark = sim.mark()
    taster.restart()
    # The boards keep their latch over the node's restart (they aren't
    # reset), so the relay goes off at start, and nothing else comes on.
    off = sim.expect(r"relay 0 off", chip=taster, kind="relay", since=mark, timeout=60)
    assert off["bank"] == 1
    sim.no_event(r" on", kind="relay", chip=taster, since=mark, within=5)
