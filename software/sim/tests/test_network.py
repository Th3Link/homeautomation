"""The network settings: stored over the REST API, they take effect after
a restart. The gateway's QEMU network (slirp) hands out 10.0.2.15/24 with
the router 10.0.2.2 and the DNS server 10.0.2.3, and the web UI's port
forwarding goes to 10.0.2.15: the address by hand is that one, so the
gateway stays reachable."""

import time

import pytest

DEFAULTS = {"ipv4_mode": "dhcp", "ipv4_address": None, "ipv4_prefix": 24,
            "ipv4_gateway": None, "dns_servers": [], "slaac": True, "dhcpv6": "auto",
            "ipv6_address": None, "ipv6_prefix": 64, "ipv6_gateway": None}
MANUAL = dict(DEFAULTS, ipv4_mode="manual", ipv4_address="10.0.2.15",
              ipv4_gateway="10.0.2.2", dns_servers=["10.0.2.3"], slaac=False,
              dhcpv6="enforce", ipv6_address="fec0::20", ipv6_gateway="fe80::2")


def restart(sim, api):
    """Restarts the gateway and returns its `network` once it answers."""
    sim.mark()
    assert api("POST", "/system/restart")[0] == 202
    sim.expect(r"network: ready", chip="gw", timeout=120)
    for _ in range(30):
        try:
            status, system = api("GET", "/system")
            if status == 200:
                return system["network"]
        except OSError:
            pass
        time.sleep(2)
    pytest.fail("the gateway didn't answer after the restart")


def origins(entries):
    return {e["address"]: e["origin"] for e in entries}


@pytest.fixture
def network_reset(sim, api):
    """Back to DHCP and SLAAC after the test."""
    yield
    if api("GET", "/network")[1]["settings"] != DEFAULTS:
        api("PUT", "/network", DEFAULTS)
        restart(sim, api)


def test_wrong_settings_are_refused(api):
    status, answer = api("PUT", "/network", dict(MANUAL, ipv4_gateway="10.0.3.1"))
    assert status == 400 and "ipv4_gateway" in answer["message"]
    status, answer = api("PUT", "/network", dict(MANUAL, dns_servers=[]))
    assert status == 400 and "dns_servers" in answer["message"]
    assert api("GET", "/network")[1]["settings"] == DEFAULTS


def test_settings_by_hand_take_effect_after_a_restart(sim, api, network_reset):
    status, answer = api("PUT", "/network", MANUAL)
    assert status == 200 and answer["restart_required"]
    net = restart(sim, api)
    assert (net["ipv4"], net["ipv4_origin"], net["ipv4_gateway"]) == ("10.0.2.15/24", "manual", "10.0.2.2")
    assert origins(net["dns_servers"]) == {"10.0.2.3": "manual"}
    # Without SLAAC: link-local and the address by hand, the gateway by hand.
    assert sorted(origins(net["ipv6"]).values()) == ["link-local", "manual"]
    assert origins(net["ipv6"])["fec0::20/64"] == "manual"
    assert net["ipv6_gateway"] == "fe80::2"
    assert (net["slaac"], net["dhcpv6_setting"], net["dhcpv6_mode"]) == (False, "enforce", "stateful")
    assert not api("GET", "/network")[1]["restart_required"]
