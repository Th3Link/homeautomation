"""QEMU's user network reaches the host's loopback at 10.0.2.2."""

import errno
import socket
import time
from datetime import datetime, timezone

import pytest


def test_sntp_retries_unsynchronised_server(start_sim, tmp_path):
    topo = tmp_path / "sntp.toml"
    topo.write_text('[[gateway]]\nname = "gw"\nbus = "vcan-test"\n'
                    'console = ["ntp-server 10.0.2.2"]\n')
    # Provisioning boots a separate guest. Only answer the running firmware.
    with start_sim(topo) as sim, socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as peer:
        try:
            peer.bind(("127.0.0.1", 123))
        except OSError as error:
            if error.errno in (errno.EACCES, errno.EADDRINUSE):
                pytest.skip("local SNTP peer needs an available UDP port 123 on loopback")
            raise
        peer.settimeout(30)
        for stratum in (0, 2):
            request, client = peer.recvfrom(128)
            assert len(request) == 48 and request[0] == 0x23
            # A stratum of zero must leave the clock unset and trigger a retry.
            reply = bytearray(48)
            reply[0] = 0x24
            reply[1] = stratum
            reply[24:32] = request[40:48]
            unix = int(time.time())
            reply[40:44] = (unix + 2_208_988_800).to_bytes(4, "big")
            peer.sendto(reply, client)
        timestamp = datetime.fromtimestamp(unix, timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
        sim.expect(f"sntp: time is {timestamp} UTC", chip="gw", kind="serial", since=0)
