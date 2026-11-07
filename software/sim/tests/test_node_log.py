"""A node's log over CAN (ADR 0026): the gateway downloads it with
LogDownload, and the node drops what was downloaded, so a line comes
once."""

import time

import pytest


def download(api, device):
    """Downloads `device`'s log through the gateway; the answer of the last
    read, with all of the text."""
    assert api("POST", f"/devices/{device}/log")[0] == 202
    for _ in range(60):
        time.sleep(0.5)
        status, log = api("GET", f"/devices/{device}/log")
        assert status == 200
        if log["state"] != "running":
            break
    else:
        pytest.fail("the download didn't end")
    text, start = log["text"], log["next"]
    while True:
        status, more = api("GET", f"/devices/{device}/log?from={start}")
        if not more["text"]:
            return dict(log, text=text)
        text += more["text"]
        start = more["next"]


def test_a_node_log_comes_once(sim, api, taster):
    # The node logged its start; a console line adds one we know.
    taster.console("sensor-interval 10")
    first = download(api, "taster")
    assert first["state"] == "done", first
    lines = first["text"].splitlines()
    assert lines and all(line.startswith("[") for line in lines), lines[:3]
    assert any("reset reason" in line for line in lines) or len(lines) > 3

    # Downloaded lines left the node: the next download has only newer ones.
    second = download(api, "taster")
    assert second["state"] == "done", second
    assert not set(second["text"].splitlines()) & set(lines)

