"""The gateway's REST API, and that its requests fit on the main stack: a
request routed through picoserve's nested routers once took ~60 KB of it
(47 KB) and zeroed the task pools below, which esp-hal's stack guard
doesn't see (a frame that large jumps over it)."""

import base64
import json
import secrets
import ssl
import urllib.error
import urllib.request

import pytest

# Left untouched below the deepest request; ~16 KB with the handlers boxed.
MIN_STACK_MARGIN = 8 * 1024


@pytest.fixture(scope="module")
def api(sim):
    """`api(method, path, body=None)` -> (status, parsed body), with a web
    login of this run's own set on the console."""
    gw = sim["gw"]
    user, password = "simtest", secrets.token_hex(8)
    gw.console(f"web-username {user}")
    gw.console(f"web-password {password}")
    auth = "Basic " + base64.b64encode(f"{user}:{password}".encode()).decode()
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    base = sim.topology.chip("gw").url + "/api/v1"

    def call(method, path, body=None):
        req = urllib.request.Request(base + path, method=method,
                                     data=None if body is None else json.dumps(body).encode())
        req.add_header("Authorization", auth)
        if body is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, context=ctx, timeout=30) as r:
                status, text = r.status, r.read().decode()
        except urllib.error.HTTPError as e:
            status, text = e.code, e.read().decode()
        return status, json.loads(text) if text.startswith(("{", "[")) else text

    return call


RELAY = {"relay": {"device": {"type": 6, "id": 128}, "channel": 1}}
LIGHT = {"light": {"groups": [{"device": {"type": 3, "id": 6}, "mask": 1280},
                              {"device": {"type": 3, "id": 6}, "mask": 2560}],
                   "levels": [[0, 34], [17, 34], [127, 127]], "default_level": 2}}
BUTTON = {"events": [
    {"event": "single", "actions": [{"output": {"name": "simlicht", "op": "toggle"}},
                                    {"output": {"name": "simrelais", "op": "on"}}]},
    {"event": "hold", "actions": [{"output": {"name": "simlicht", "op": "inc"}}]},
]}
MOTION = {"detectors": [{"type": 4, "id": 10}], "output": "simlicht",
          "timeout_s": 60, "lockout_s": 5}


def test_automation_routes_fit_on_the_stack(api):
    _, before = api("GET", "/system")
    steps = [
        ("GET", "/automation", None),
        ("GET", "/automation/outputs", None),
        ("PUT", "/automation/outputs/simrelais", RELAY),
        ("PUT", "/automation/outputs/simlicht", LIGHT),
        ("POST", "/automation/outputs/simrelais/command", "on"),
        ("PUT", "/automation/switches/simflur", {"switches": [{"type": 4, "id": 10}]}),
        ("PUT", "/automation/switches/simflur/0", BUTTON),
        ("GET", "/automation/switches/simflur/0", None),
        ("GET", "/automation/switches", None),
        ("PUT", "/automation/devices/0x10040a00", {"features": ["pir", "nightlight"]}),
        ("GET", "/automation/devices", None),
        ("PUT", "/automation/motion/simpir", MOTION),
        ("GET", "/automation/motion", None),
        ("GET", "/automation/schedules", None),
        ("DELETE", "/automation/motion/simpir", None),
        ("DELETE", "/automation/devices/0x10040a00", None),
        ("DELETE", "/automation/switches/simflur", None),
        ("DELETE", "/automation/outputs/simlicht", None),
        ("DELETE", "/automation/outputs/simrelais", None),
    ]
    for method, path, body in steps:
        status, answer = api(method, path, body)
        assert status < 300, (method, path, status, answer)

    _, after = api("GET", "/system")
    assert after["uptime_s"] >= before["uptime_s"], "the gateway restarted"
    assert after["stack_untouched"] >= MIN_STACK_MARGIN, after["stack_untouched"]
