"""Two QEMU gateways share a real Mosquitto broker, one CAN bus each."""

import json
import re
import shutil
import socket
import subprocess
import time
import uuid
from contextlib import ExitStack
from pathlib import Path

import pytest

from simlib import can
from simlib.topology import Topology

TOPOLOGY = Path(__file__).with_name("mqtt.toml")
HOST = "127.0.0.1"


class Broker:
    def __init__(self, directory, port):
        self.port = port
        self.log = directory / "mosquitto.log"
        self.config = directory / "mosquitto.conf"
        self.config.write_text(f"listener {port} {HOST}\nallow_anonymous true\npersistence false\n")
        self.process = None
        self.epoch = 0

    def start(self):
        with self.log.open("a") as output:
            self.epoch = output.tell()
            self.process = subprocess.Popen(
                ["mosquitto", "-c", self.config, "-v"], stdout=output, stderr=subprocess.STDOUT,
            )
        self.wait_log(lambda line: "mosquitto version" in line and "running" in line, timeout=5)

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()

    def wait_log(self, predicate, *, timeout=30):
        deadline = time.monotonic() + timeout
        with self.log.open() as log:
            log.seek(self.epoch)
            while time.monotonic() < deadline:
                for line in log:
                    if predicate(line):
                        return
                if self.process.poll() is not None:
                    pytest.fail(f"Mosquitto exited with code {self.process.returncode}. See {self.log}")
                time.sleep(0.05)
        pytest.fail(f"No matching MQTT event within {timeout}s. See {self.log}")

    def wait_gateway(self, gateway):
        client = f"gateway-{gateway.topology.mac.hex()}"
        self.wait_log(lambda line: f"Received PUBLISH from {client} " in line
                      and "'canbus/available/" in line)

    def publish(self, topic, payload):
        subprocess.run(
            ["mosquitto_pub", "-V", "mqttv5", "-h", HOST, "-p", str(self.port),
             "-t", topic, "-m", payload],
            check=True, capture_output=True, text=True, timeout=10,
        )

    def receive(self, topic, *, action=None):
        client_id = f"sim-test-{uuid.uuid4().hex}"
        client = subprocess.Popen(
            ["mosquitto_sub", "-V", "mqttv5", "-h", HOST, "-p", str(self.port),
             "-t", topic, "-C", "1", "-W", "35", "-F", "%r %p", "-i", client_id],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            if action is not None:
                self.wait_log(lambda line: f"Sending SUBACK to {client_id}" in line, timeout=5)
                action()
            output, error = client.communicate(timeout=40)
            assert client.returncode == 0, (topic, error, self.log)
            retained, payload = output.rstrip("\n").split(" ", 1)
            return retained == "1", payload
        finally:
            if client.poll() is None:
                client.kill()
                client.communicate()

    def config_payload(self, topic):
        self.wait_log(lambda line: "Received PUBLISH from gateway-" in line
                      and ", r1," in line and f"'{topic}'" in line)
        retained, payload = self.receive(topic)
        assert retained, topic
        return json.loads(payload)


@pytest.fixture(scope="session")
def mqtt_setup(tmp_path_factory, start_sim):
    for program in ("mosquitto", "mosquitto_pub", "mosquitto_sub"):
        if not shutil.which(program):
            pytest.skip(f"{program} is required for MQTT tests")
    if not all(can.interface_exists(bus) for bus in Topology(TOPOLOGY).buses):
        pytest.skip("Create the MQTT buses with ./sim -f tests/mqtt.toml net")

    directory = tmp_path_factory.mktemp("mqtt")
    print(f"MQTT test logs and flash images: {directory}")
    with ExitStack() as reservations:
        ports = []
        for _ in range(5):
            listener = reservations.enter_context(socket.socket())
            listener.bind((HOST, 0))
            ports.append(listener.getsockname()[1])
    broker = Broker(directory, ports[0])
    web_ports = iter(ports[1:])
    text = TOPOLOGY.read_text().replace("10.0.2.2:18883", f"10.0.2.2:{broker.port}")
    topology = directory / "mqtt.toml"
    topology.write_text(re.sub(r"(?m)^(https?) = \d+$",
                               lambda match: f"{match[1]} = {next(web_ports)}", text))

    try:
        broker.start()
        with start_sim(topology) as sim:
            try:
                broker.wait_gateway(sim["gw-a"])
                broker.wait_gateway(sim["gw-b"])
                yield sim, broker
            finally:
                (directory / "can.jsonl").write_text("".join(
                    json.dumps(event) + "\n" for event in sim.events() if event.kind == "can"
                ))
    finally:
        broker.stop()


def test_mqtt_commands_and_state_on_two_can_buses(mqtt_setup):
    sim, broker = mqtt_setup
    for name in ("tastera", "tasterb"):
        assert broker.receive(f"canbus/button/{name}",
                              action=lambda: sim[name].press(1, "double")) == (False, "1/double/0")

    relay, other = sim["relaya"], sim["relayb"]
    assert not any(relay.relays) and not any(other.relays)
    start = sim.mark()
    assert broker.receive("canbus/relais_state/relaya/0/0",
                          action=lambda: broker.publish("canbus/relais_command/relaya", "0/3/0")) == (False, "3")
    frame = sim.expect_can("Relais", chip=relay, data=[0, 3, 0, 0, 0, 0], since=start)
    assert frame.bus == relay.topology.bus
    sim.no_event(kind="can", since=start, within=1,
                 where=lambda frame: frame.bus == other.topology.bus and frame.message == "Relais")
    assert relay.relays[0] and not any(other.relays)
    assert broker.receive("canbus/relais_state/relaya/0/0",
                          action=lambda: broker.publish("canbus/relais_command/relaya", "0/0/0")) == (False, "0")
    assert not relay.relays[0]


def assert_discovery(broker, name, device_key):
    device = f"canbus_{device_key:08x}"
    sensor = broker.config_payload(f"homeassistant/sensor/{device}/temperature_68000077/config")
    assert sensor["dev"]["ids"] == [device] and sensor["dev"]["name"] == name
    assert sensor["uniq_id"] == f"{device}_temperature_68000077"
    assert sensor["stat_t"] == f"canbus/temperature/{name}/0x68000077"
    assert (sensor["dev_cla"], sensor["stat_cla"], sensor["unit_of_meas"]) == (
        "temperature", "measurement", "°C",
    )
    trigger = broker.config_payload(
        f"homeassistant/device_automation/{device}/button1_double/config",
    )
    assert trigger["dev"]["ids"] == [device] and trigger["dev"]["name"] == name
    assert trigger["t"] == f"canbus/button/{name}"
    assert (trigger["atype"], trigger["type"], trigger["stype"], trigger["pl"]) == (
        "trigger", "button_double_press", "button_2", "1/double",
    )
    return sensor["stat_t"]


def test_home_assistant_discovery_survives_broker_restart(mqtt_setup):
    sim, broker = mqtt_setup
    switches = {"tastera": 0x10040A00, "tasterb": 0x10041400}
    for name, device_key in switches.items():
        switch = sim[name]
        switch.bme680.set(temperature=27.25)
        sim.expect_can("TemperatureSensor", chip=switch, sensor="bme680@77", value=27.25)
        assert broker.receive(f"canbus/button/{name}",
                              action=lambda: switch.press(1, "double")) == (False, "1/double/0")
        sensor_topic = assert_discovery(broker, name, device_key)
        retained, value = broker.receive(sensor_topic)
        assert not retained and float(value) == 27.25

    # Persistence is off. A new broker starts with an empty retained store.
    broker.stop()
    broker.start()
    for gateway in sim.topology.gateways:
        broker.wait_gateway(sim[gateway.name])
    for name, device_key in switches.items():
        assert_discovery(broker, name, device_key)
