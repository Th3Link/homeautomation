"""pytest against the simulation: `sim` starts tests/sim.toml (or uses it
if it already runs) and stops it at the end; the chips are fixtures by
name. Run from software/sim:

    python -m pytest tests            # -k button, -x, …
    python -m pytest tests --sim-keep # leave the simulation running

The bus has to exist: ./sim -f tests/sim.toml net
"""

import subprocess
import sys
from pathlib import Path

import pytest

TESTS = Path(__file__).resolve().parent
sys.path.insert(0, str(TESTS.parent))

from simlib import Sim, can, supervisor  # noqa: E402
from simlib.topology import Topology  # noqa: E402

SIM = TESTS.parent / "sim"


def pytest_addoption(parser):
    group = parser.getgroup("sim")
    group.addoption("--sim-topology", default=str(TESTS / "sim.toml"),
                    help="the installation to test (default: tests/sim.toml)")
    group.addoption("--sim-keep", action="store_true",
                    help="leave the simulation running after the tests")


@pytest.fixture(scope="session")
def sim(request):
    """The running simulation (simlib/client.py)."""
    path = request.config.getoption("--sim-topology")
    topo = Topology(path)
    missing = [b for b in topo.buses if not can.interface_exists(b)]
    if missing:
        pytest.exit(f"no CAN interface {', '.join(missing)}: ./sim -f {path} net", returncode=2)
    started = False
    if supervisor.request(topo, {"cmd": "status"}, 2) is None:
        subprocess.run([sys.executable, str(SIM), "-f", path, "up", "-d"], check=True)
        started = True
    s = Sim(path)
    try:
        s.wait_ready(timeout=180)
        yield s
    finally:
        s.close()
        if started and not request.config.getoption("--sim-keep"):
            supervisor.request(topo, {"cmd": "down"}, 10)


@pytest.fixture(autouse=True)
def _mark(request):
    """Each test only sees the events from its own start on."""
    if "sim" in request.fixturenames:
        request.getfixturevalue("sim").mark()


@pytest.fixture
def taster(sim):
    return sim["taster"]


@pytest.fixture
def relais(sim):
    return sim["relais"]


@pytest.fixture
def pwm(sim):
    return sim["pwm"]
