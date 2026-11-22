"""Run through memory_profile.py with a memory-profile gateway build."""

import json
import os
import ssl
import time
from pathlib import Path

import pytest

from memory_profile import IDLE_SECONDS, parse_serial_sample, rest_summary, sample_rest

pytestmark = pytest.mark.skipif(not os.environ.get("SIM_MEMORY_PROFILE"),
                                reason="run with memory_profile.py")


def next_sample(sim, since=None):
    event = sim.expect(r"memory: \d+", chip="gw", kind="serial", since=since, timeout=15)
    return event, parse_serial_sample(event.text)


def test_idle_memory(sim):
    time.sleep(IDLE_SECONDS)
    _, memory = next_sample(sim)
    assert memory["heap_size"] >= memory["heap_peak_used"] >= memory["heap_used"]
    assert memory["stack_size"] >= memory["stack_untouched"]


def test_pending_sntp_lookup(sim):
    ready = sim.expect("network: ready", chip="gw", kind="serial", since=0)
    _, memory = next_sample(sim, since=ready.seq)
    assert memory["heap_used"] == 0, memory


def test_rest_memory(sim):
    gateway = sim["gw"].topology
    time.sleep(5)
    capture = sample_rest(gateway.url, "memory", "profile-test-only", IDLE_SECONDS,
                          tls=ssl._create_unverified_context())
    (Path(os.environ["SIM_MEMORY_PROFILE"]) / "rest.json").write_text(json.dumps(capture, indent=2))
    measured = rest_summary(capture)
    assert not measured["errors"], measured["errors"]
    assert len(measured["boots"]) == 1 and measured["boots"][0]["samples"] >= 2


def test_transient_heap_allocation(sim):
    _, before = next_sample(sim)
    assert "memory probe: allocated and freed 8192 bytes" in sim["gw"].console("memory-probe")
    _, after = next_sample(sim, since=sim.mark())
    assert after["boot_id"] == before["boot_id"]
    assert after["heap_peak_used"] > before["heap_peak_used"]
    assert after["heap_peak_used"] >= before["heap_used"] + 8192
    assert after["heap_used"] == before["heap_used"]
