"""Two firmware versions, fresh virtual flash and the gateway's real OTA routes."""

import base64
import binascii
import http.client
import json
import socket
import ssl
import time
from pathlib import Path

import pytest

from simlib import Timeout, can
from simlib.topology import Topology

TOPOLOGY = Path(__file__).with_name("ota.toml")
HOST = "127.0.0.1"
OLD = "qemu-v1"
NEW = "qemu-v2"


class FirmwareApi:
    def __init__(self, port):
        self.port = port

    def request(self, method, path, *, data=None, timeout=5):
        connection = http.client.HTTPSConnection(HOST, self.port, timeout=timeout,
                                                 context=ssl._create_unverified_context())
        credentials = base64.b64encode(b"ota-test:qemu-test-only").decode()
        headers = {"Authorization": f"Basic {credentials}", "Connection": "close"}
        if data is not None:
            headers["Content-Type"] = "application/octet-stream"
        try:
            connection.request(method, f"/api/v1{path}", body=data, headers=headers)
            response = connection.getresponse()
            body = response.read().decode()
            if response.getheader("Content-Type", "").startswith("application/json"):
                body = json.loads(body)
            return response.status, body
        finally:
            connection.close()

    def get(self, path):
        status, body = self.request("GET", path)
        assert status == 200, (path, status, body)
        return body

    def upload(self, path, data, *, wrong_crc=False):
        crc = binascii.crc32(data) ^ int(wrong_crc)
        start = time.monotonic()
        result = self.request("PUT", f"{path}?crc32={crc}", data=data, timeout=900)
        if result[0] == 200:
            elapsed = time.monotonic() - start
            print(f"{path}: {len(data)} bytes accepted in {elapsed:.2f} s ({len(data) / elapsed / 1024:.1f} KiB/s)")
        return result

    def wait_gateway_version(self, version, *, timeout=90):
        deadline = time.monotonic() + timeout
        last = None
        while time.monotonic() < deadline:
            try:
                last = status, body = self.request("GET", "/system")
                if status == 200 and body["firmware_version"] == version:
                    return
            except (OSError, http.client.HTTPException) as exc:
                last = exc
            time.sleep(0.5)
        pytest.fail(f"Gateway did not report {version!r} within {timeout}s. Last response: {last!r}")


@pytest.fixture(scope="session")
def ota_images(pytestconfig):
    directory = pytestconfig.getoption("--ota-images")
    if directory is None:
        pytest.skip("build OTA fixtures with tests/build-ota-images.sh and pass --ota-images")
    directory = directory.resolve()
    for binary in ("gateway-hardware", "cancomponents"):
        for path in (directory / OLD / binary, directory / NEW / f"{binary}.bin"):
            if not path.is_file():
                pytest.fail(f"Missing OTA fixture {path}. Run tests/build-ota-images.sh")
    if not all(can.interface_exists(bus) for bus in Topology(TOPOLOGY).buses):
        pytest.fail("Create the OTA bus with ./sim -f tests/ota.toml net")
    return directory


def wait_for_web_boot(simulation, *, since):
    simulation.expect(r"network: ready", chip="gw", kind="serial", since=since, timeout=60)
    # Avoid making the first HTTP request while the network tasks are starting.
    time.sleep(5)


@pytest.fixture
def ota_setup(ota_images, start_sim, tmp_path):
    with socket.socket() as https:
        https.bind((HOST, 0))
        https_port = https.getsockname()[1]
    old = ota_images / OLD
    firmware = (
        f"[firmware]\ngateway = {json.dumps(str(old / 'gateway-hardware'))}\n"
        f"node = {json.dumps(str(old / 'cancomponents'))}\n\n"
    )
    topology = tmp_path / "ota.toml"
    topology.write_text(firmware + TOPOLOGY.read_text().replace(
        'name = "gw"', f'name = "gw"\nhttps = {https_port}', 1,
    ))
    print(f"OTA flash images and simulator logs: {Topology(topology).state_dir}")
    with start_sim(topology) as simulation:
        wait_for_web_boot(simulation, since=0)
        yield simulation, FirmwareApi(https_port)


def test_gateway_update_rejects_bad_crc_then_preserves_config(ota_setup, ota_images):
    simulation, api = ota_setup
    api.wait_gateway_version(OLD)
    config = api.get("/config")
    assert config["hostname"] == "ota-gateway"
    assert config["web_username"] == "ota-test"

    image = (ota_images / NEW / "gateway-hardware.bin").read_bytes()
    status, error = api.upload("/firmware/gateway", image[:4096], wrong_crc=True)
    assert (status, error["error"]) == (422, "verify")
    job = api.get("/firmware/job")
    assert (job["state"], job["target"], job["error"]) == ("failed", "gateway", error["message"])
    assert api.get("/system")["firmware_version"] == OLD

    start = simulation.mark()
    assert api.upload("/firmware/gateway", image) == (200, {"bytes": len(image), "restarting": True})
    wait_for_web_boot(simulation, since=start)
    api.wait_gateway_version(NEW)
    assert api.get("/config") == config


def assert_node_reports(simulation, node, version, *, timeout=10):
    # Check a live CAN reply instead of the gateway's cached device table.
    simulation.send_can("RequestParameter", to=node, rtr=True)
    simulation.expect_can("ApplicationVersionString", chip=node, timeout=timeout,
                          data=version.encode().ljust(8, b"\0"))


def assert_node_rediscovered(simulation, *, since):
    announcement = simulation.expect_can("Discovery", chip="taster", data=b"taster",
                                         since=since, timeout=10)
    simulation.expect_can("DiscoveryAck", chip="taster", since=announcement.seq, timeout=10)
    assert_node_reports(simulation, "taster", NEW)


def test_addressed_node_update_rejects_bad_crc_and_leaves_peer_unchanged(ota_setup, ota_images):
    simulation, api = ota_setup
    for node in ("taster", "spare"):
        assert_node_reports(simulation, node, OLD)

    image = (ota_images / NEW / "cancomponents.bin").read_bytes()
    status, error = api.upload("/devices/taster/firmware", image[:128], wrong_crc=True)
    assert (status, error["error"]) == (502, "update_failed")
    assert error["message"] == "the device could not verify the image"
    job = api.get("/firmware/job")
    assert (job["state"], job["target"], job["protocol"], job["error"]) == (
        "failed", "device", "blocks", error["message"],
    )
    assert_node_reports(simulation, "taster", OLD)

    start = simulation.mark()
    assert api.upload("/devices/taster/firmware", image) == (200, {"bytes": len(image), "restarting": False})
    job = api.get("/firmware/job")
    assert (job["state"], job["target"], job["protocol"], job["sent"], job["total"], job["error"]) == (
        "done", "device", "blocks", len(image), len(image), None,
    )
    boot = simulation.expect(r"Loaded app from partition at offset 0x200000\b",
                             chip="taster", kind="serial", since=start, timeout=45)
    try:
        assert_node_rediscovered(simulation, since=boot.seq)
    except Timeout:
        # The current QEMU CAN model can stop forwarding after a guest reset.
        # Require the OTA boot and an announcement attempt before power-cycling.
        simulation.expect(r"msg: Discovery.*is_remote: false", chip="taster",
                          kind="serial", since=boot.seq, timeout=10)
        print("Restarting the QEMU process to reconnect CAN, keeping the OTA flash")
        restart = simulation.mark()
        simulation["taster"].restart()
        assert_node_rediscovered(simulation, since=restart)
    assert_node_reports(simulation, "spare", OLD)
