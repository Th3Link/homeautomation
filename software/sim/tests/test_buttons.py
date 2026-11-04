"""Buttons and the PIR: what the node makes of presses on its pins."""

import pytest

# ButtonEvent: [button, state, count u16]; states in simlib/can.py.
STATES = {"single": 3, "double": 4, "triple": 5}


@pytest.mark.parametrize("kind", ["single", "double", "triple"])
def test_clicks(sim, taster, kind):
    taster.press(1, kind)
    sim.expect_can("ButtonEvent", chip=taster, data=[1, STATES[kind]], timeout=5)


def test_hold_repeats_until_released(sim, taster):
    # The first repeat after 1.5 s, then every 0.5 s.
    taster.press(2, "hold", secs=3.2)
    first = sim.expect_can("ButtonEvent", chip=taster, data=[2, 2])
    sim.expect_can("ButtonEvent", chip=taster, data=[2, 2], since=first.seq)
    sim.expect_can("ButtonEvent", chip=taster, data=[2, 0], timeout=5)


def test_extension_buttons_are_4_to_7(sim, relais):
    relais.press(5)
    sim.expect_can("ButtonEvent", chip=relais, data=[5, STATES["single"]], timeout=5)


def test_motion_comes_and_goes(sim, taster):
    taster.motion(3)
    sim.expect_can("PirSensor", chip=taster, timeout=5,
                   data=[8, 1])
    sim.expect("motion over", chip=taster, kind="can", timeout=10)


def test_a_button_the_board_lacks_is_refused(sim, taster):
    from simlib import SimError
    with pytest.raises(SimError, match="no button 6"):
        taster.press(6)
