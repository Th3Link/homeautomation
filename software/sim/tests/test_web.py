"""The gateway's REST API, and that its requests fit on the main stack: a
request routed through picoserve's nested routers once took ~60 KB of it
(47 KB) and zeroed the task pools below, which esp-hal's stack guard
doesn't see (a frame that large jumps over it)."""

# Left untouched below the deepest request; ~14 KB with the handlers boxed.
MIN_STACK_MARGIN = 8 * 1024

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
