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
        if method == "GET" and path.endswith("/simflur/0"):
            assert answer == BUTTON
        if method == "PUT" and path.endswith("/simflur/0"):
            _, saved = api("GET", "/system")
            assert saved["boot_id"] == before["boot_id"]
            print(f"Stack untouched after button save: {saved['stack_untouched']} bytes")
            assert saved["stack_untouched"] >= MIN_STACK_MARGIN

    _, after = api("GET", "/system")
    assert after["boot_id"] == before["boot_id"], "the gateway restarted"
    print(f"Automation stack untouched: {after['stack_untouched']} of {after['stack_size']} bytes")
    assert after["stack_untouched"] >= MIN_STACK_MARGIN, after["stack_untouched"]


def test_output_state_survives_reload_only_for_the_same_kind(sim, api):
    path = "/automation/outputs"
    name = "reloadlight"

    def level():
        status, result = api("GET", path)
        assert status == 200
        return next(item["level"] for item in result["items"] if item["name"] == name)

    def put_and_wait(output, config):
        since = sim.mark()
        status, result = api("PUT", f"{path}/{output}", config)
        assert status < 300, result
        sim.expect(r"automation: \d+ outputs,", chip="gw", kind="serial", since=since)

    try:
        put_and_wait(name, LIGHT)
        since = sim.mark()
        assert api("POST", f"{path}/{name}/command", "on")[0] < 300
        sim.expect_can("LampGroup", data=[17], since=since)
        assert level() == LIGHT["light"]["default_level"]
        put_and_wait("reloadgroup", {"group": {"members": [name]}})
        assert level() == LIGHT["light"]["default_level"]
        put_and_wait(name, RELAY)
        assert level() == 0
    finally:
        api("DELETE", f"{path}/reloadgroup")
        api("DELETE", f"{path}/{name}")
