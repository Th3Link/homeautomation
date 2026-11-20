"""The gateway's scan discovers all three nodes on the test bus."""


def test_scan_announces_and_acknowledges_each_node(sim, taster, relais, pwm):
    gateway = sim["gw"]
    bus = gateway.topology.bus
    start = sim.mark()
    assert "discovery requested" in gateway.console("discover")

    scan = sim.expect(
        kind="can", since=start, timeout=10,
        where=lambda frame: frame.bus == bus and frame.id == 0x1000002B and frame.rtr,
    )

    for node in (taster, relais, pwm):
        name = node.name.encode()
        announcement = sim.expect_can("Discovery", chip=node, data=name, since=scan.seq, timeout=10)
        assert not announcement.rtr and announcement.payload == name

        ack = sim.expect_can("DiscoveryAck", chip=node, since=announcement.seq, timeout=10)
        assert not ack.rtr and ack.payload == b""
        assert (ack.id & ~0xFF) == (announcement.id & ~0xFF)
