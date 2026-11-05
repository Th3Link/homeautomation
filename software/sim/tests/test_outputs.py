"""What the gateway's commands switch: relays, the night light, PWM."""

import pytest


# Relais: [relay, state (3 on, 0 off), duration ms (24 bits LE), bank].
@pytest.mark.parametrize("relay", [0, 7, 11])
def test_relay_on_and_off(sim, relais, relay):
    sim.send_can("Relais", to=relais, data=[relay, 3, 0, 0, 0, 0])
    sim.expect(rf"relays? .*\b{relay}\b.* on", chip=relais, kind="relay", timeout=5)
    assert relais.relays[relay]
    sim.send_can("Relais", to=relais, data=[relay, 0, 0, 0, 0, 0])
    sim.expect(rf"relays? .*\b{relay}\b.* off", chip=relais, kind="relay", timeout=5)
    assert not relais.relays[relay]


@pytest.mark.parametrize("level", [3, 1, 0])
def test_night_light(sim, taster, level):
    sim.send_can("Nightlight", to=taster, data=[level])
    sim.expect(chip=taster, kind="device", timeout=5,
               where=lambda e: e["state"]["level"] == level)
    assert taster.nightlight.level == level


def test_pwm_channels_of_a_bank(sim, pwm):
    # LampGroup: value, channel bitmask (24 bits LE), bank.
    sim.send_can("LampGroup", to=pwm, data=[255, 0b1010, 0, 0, 2])
    sim.expect(chip=pwm, kind="device", timeout=5,
               where=lambda e: e["device"] == "pwm-2" and e["state"]["duty"][3] == 100)
    duty = pwm.part("pwm-2").duty
    assert duty[1] == duty[3] == 100
    assert duty[0] == duty[2] == 0
    sim.send_can("LampGroup", to=pwm, data=[0, 0b1010, 0, 0, 2])
    sim.expect(chip=pwm, kind="device", timeout=5,
               where=lambda e: e["device"] == "pwm-2" and not any(e["state"]["duty"]))


def test_relay_on_for_a_while(sim, relais):
    sim.send_can("Relais", to=relais, data=[4, 3, 0xDC, 0x05, 0, 0])  # 1500 ms
    on = sim.expect(r"relay 4 on", chip=relais, kind="relay", timeout=5)
    off = sim.expect(r"relay 4 off", chip=relais, kind="relay", timeout=5)
    assert 1.2 < off["t"] - on["t"] < 2.5


def test_console(relais):
    assert "relais 0/2 -> On" in relais.console("relais 2 on")
    relais.console("relais 2 off")
