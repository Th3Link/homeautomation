#!/usr/bin/env python3
"""Collect per-boot gateway memory data from QEMU workloads or /api/v1/system."""

import argparse
import base64
import http.client
import json
import os
import re
import shutil
import ssl
import subprocess
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

from simlib import supervisor
from simlib.images import default_elf, sha256
from simlib.topology import REPO, SIM_DIR, Topology, TopologyError

IDLE_SECONDS = 10
SAMPLE_SECONDS = 2
MEMORY = re.compile(r"memory: (\d+) (\d+) (\d+) (\d+) (\d+) (\d+) (\d+)(?: (\d+) (\d+) (\d+) (\d+))?\s*$")
FIELDS = ("boot_id", "uptime_ms", "heap_size", "heap_used", "heap_peak_used",
          "stack_size", "stack_untouched")
CRYPTO_FIELDS = ("size", "used", "peak", "failures")
SCENARIOS = {
    "idle": ("idle.toml", "test_memory.py::test_idle_memory"),
    "idle-rest": ("idle.toml", "test_memory.py::test_rest_memory"),
    "heap-probe": ("idle.toml", "test_memory.py::test_transient_heap_allocation"),
    "can-mqtt": ("mqtt.toml", "test_mqtt.py::test_mqtt_commands_and_state_on_two_can_buses"),
    "ha-reconnect": ("mqtt.toml", "test_mqtt.py::test_home_assistant_discovery_survives_broker_restart"),
    "dual-mqtt": ("mqtt.toml", "test_mqtt.py::test_secondary_commands_during_five_https_downloads"),
    "dual-mqtt-tls": ("mqtt.toml", "test_mqtt.py::test_secondary_commands_during_five_https_downloads"),
    "https-cert-rotation": ("idle.toml", "test_tls_memory.py::test_replace_active_certificate"),
    "https-automation": ("sim.toml", "test_web.py"),
    "gateway-ota": ("ota.toml", "test_firmware_update.py::test_gateway_update_rejects_bad_crc_then_preserves_config"),
    "can-ota": ("ota.toml", "test_firmware_update.py::test_addressed_node_update_rejects_bad_crc_and_leaves_peer_unchanged"),
    "mqtt-tls": ("mqtt.toml", "test_mqtt.py::test_mqtt_commands_and_state_on_two_can_buses"),
    "combined": ("mqtt.toml", "test_mqtt.py::test_active_nodes_with_https_and_mqtt"),
}


def parse_serial_sample(line):
    match = MEMORY.search(line)
    if not match:
        return None
    values = match.groups()
    sample = dict(zip(FIELDS, map(int, values[:len(FIELDS)])))
    crypto = values[len(FIELDS):]
    if crypto[0] is not None:
        sample["crypto_pool"] = dict(zip(CRYPTO_FIELDS, map(int, crypto)))
    return sample


def collect_serial(path):
    boots = []
    current = None
    for line in path.read_text(errors="replace").splitlines():
        if "gateway-hardware booting" in line:
            current = {"samples": [], "panic": False}
            boots.append(current)
        sample = parse_serial_sample(line)
        if sample is not None:
            if current is None or (current["samples"] and (
                sample["boot_id"] != current["samples"][-1]["boot_id"]
                or sample["uptime_ms"] < current["samples"][-1]["uptime_ms"]
            )):
                current = {"samples": [], "panic": False}
                boots.append(current)
            current["samples"].append(sample)
        if current is not None:
            if "reset reason: " in line:
                current["reset_reason"] = line.split("reset reason: ", 1)[1]
            if "PANIC" in line or "Stack overflow" in line:
                current["panic"] = True
    return [dict(summarize(boot.pop("samples")), **boot) for boot in boots]


def summarize(samples):
    if not samples:
        return {"samples": 0}
    heap_peak = max(s["heap_peak_used"] for s in samples)
    stack_free = min(s["stack_untouched"] for s in samples)
    first, last = samples[0], samples[-1]
    result = dict(samples=len(samples), boot_id=last["boot_id"],
                start_uptime_ms=first["uptime_ms"], end_uptime_ms=last["uptime_ms"],
                heap_size=last["heap_size"], heap_peak_used=heap_peak,
                heap_min_free=last["heap_size"] - heap_peak,
                stack_size=last["stack_size"], stack_peak_used=last["stack_size"] - stack_free,
                stack_untouched=stack_free)
    if "crypto_pool" in last:
        result["crypto_pool"] = last["crypto_pool"]
    return result


def sample_rest(url, user, password, seconds, *, cafile=None, tls=None):
    auth = base64.b64encode(f"{user}:{password}".encode()).decode()
    request = urllib.request.Request(url.rstrip("/") + "/api/v1/system",
                                     headers={"Authorization": f"Basic {auth}"})
    if tls is None and request.type == "https":
        tls = ssl.create_default_context(cafile=cafile)
    start = time.monotonic()
    samples, errors = [], []
    next_sample = start
    while next_sample < start + seconds:
        time.sleep(max(0, next_sample - time.monotonic()))
        elapsed_ms = round(1000 * (time.monotonic() - start))
        try:
            with urllib.request.urlopen(request, context=tls, timeout=3) as response:
                value = json.load(response)
            if not isinstance(value, dict):
                raise ValueError("/system did not return a JSON object")
            sample = {key: value[key] for key in FIELDS}
            if any(type(sample[key]) is not int or sample[key] < 0 for key in FIELDS):
                raise ValueError("invalid memory fields")
            if "crypto_pool" in value:
                sample["crypto_pool"] = value["crypto_pool"]
            samples.append(dict(sample, elapsed_ms=elapsed_ms, firmware_version=value["firmware_version"]))
        except (OSError, ValueError, KeyError, http.client.HTTPException) as exc:
            errors.append(dict(elapsed_ms=elapsed_ms, error=str(exc)))
        next_sample = max(next_sample + SAMPLE_SECONDS, time.monotonic())
    return dict(samples=samples, errors=errors, duration_s=seconds)


def rest_summary(capture):
    boots = []
    for sample in capture["samples"]:
        if (not boots or sample["boot_id"] != boots[-1][-1]["boot_id"]
                or sample["uptime_ms"] < boots[-1][-1]["uptime_ms"]):
            boots.append([])
        boots[-1].append(sample)
    return {"boots": [summarize(boot) for boot in boots], "errors": capture["errors"],
            "duration_s": capture["duration_s"]}


def test_result(junit, returncode):
    try:
        cases = list(ET.parse(junit).iter("testcase"))
    except (ET.ParseError, OSError):
        return "aborted"
    if not cases or all(case.find("skipped") is not None for case in cases):
        return "skipped" if returncode == 0 else "failed"
    return "passed" if returncode == 0 else "failed"


def run_scenario(name, directory, ota_images):
    template, test = SCENARIOS[name]
    directory.mkdir(parents=True)
    template = SIM_DIR / "tests" / template
    topology = directory / template.name
    topology.write_bytes(template.read_bytes())
    junit = directory / "pytest.xml"
    args = [sys.executable, "-m", "pytest", "-q", "-s", "-p", "no:cacheprovider",
            "--rootdir=.", "--sim-topology", str(topology), "--basetemp", str(directory / "pytest"),
            "--junitxml", str(junit), f"tests/{test}"]
    env = dict(os.environ, SIM_MEMORY_PROFILE=str(directory), PYTEST_DISABLE_PLUGIN_AUTOLOAD="1")
    if name in ("mqtt-tls", "dual-mqtt-tls"):
        env["SIM_MQTT_TLS"] = "1"
    if name.endswith("-ota") and ota_images:
        args.extend(("--ota-images", str(ota_images)))
    start = time.monotonic()
    try:
        with (directory / "pytest.log").open("w") as log:
            result = subprocess.run(args, cwd=SIM_DIR, env=env, stdout=log, stderr=subprocess.STDOUT,
                                    timeout=1100 if name.endswith("-ota") else 300)
        status = test_result(junit, result.returncode)
    except subprocess.TimeoutExpired:
        status = "aborted"
    finally:
        # Fixtures normally stop the supervisor. Recover it if pytest was interrupted.
        for config in (topology, *directory.glob("pytest/**/*.toml")):
            try:
                supervisor.request(Topology(config), {"cmd": "down"}, 10)
            except TopologyError as exc:
                print(exc, file=sys.stderr)

    gateways = []
    for path in sorted(directory.glob("**/.sim/*/chips/*/serial.log")):
        config = path.parents[4] / (path.parents[2].name + ".toml")
        topo = Topology(config)
        chip = topo.chip(path.parent.name)
        if chip.is_gateway:
            elf = topo.firmware.get("gateway", default_elf("gateway"))
            gateways.append(dict(name=chip.name, boots=collect_serial(path),
                                 serial_log=str(path.relative_to(directory)),
                                 topology=str(config.relative_to(directory)),
                                 topology_sha256=sha256(config),
                                 elf_sha256=sha256(elf) if elf.is_file() else None))
    expected_boots = 2 if name == "gateway-ota" else 1
    partial = (not gateways or any(
        len(g["boots"]) != expected_boots or any(boot["panic"] or not boot["samples"] for boot in g["boots"])
        for g in gateways
    ))
    row = dict(scenario=name, test=f"tests/{test}", status=status, partial=partial or status != "passed",
               duration_s=round(time.monotonic() - start, 2), gateways=gateways)
    final_samples = directory / "final-samples.json"
    if final_samples.is_file():
        row["missing_final_samples"] = json.loads(final_samples.read_text())["missing"]
        row["partial"] |= bool(row["missing_final_samples"])
    else:
        row["partial"] = True
    if name.endswith("-ota") and ota_images:
        binary = "gateway-hardware" if name == "gateway-ota" else "cancomponents"
        path = ota_images / "qemu-v2" / f"{binary}.bin"
        if path.is_file():
            row["ota_binary_sha256"] = sha256(path)
    rest = directory / "rest.json"
    if rest.is_file():
        row["rest"] = rest_summary(json.loads(rest.read_text()))
        row["partial"] |= bool(row["rest"]["errors"] or len(row["rest"]["boots"]) != 1)
    return row


def qemu(args):
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sha = subprocess.check_output(["git", "-C", REPO, "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.run(["git", "-C", REPO, "diff", "--quiet", "HEAD"]).returncode != 0
    broker = None
    if shutil.which("mosquitto"):
        lines = subprocess.run(["mosquitto", "-h"], capture_output=True, text=True).stdout.splitlines()
        broker = lines[0] if lines else None
    report = dict(schema=1, platform="qemu", firmware_sha=sha, source_dirty=dirty, build_profile="release",
                  build_features=["memory-profile"], qemu_revision=args.qemu_revision,
                  broker_version=broker, sample_interval_s=SAMPLE_SECONDS,
                  date=datetime.now(timezone.utc).isoformat(), profiles=[])
    for run in range(1, args.runs + 1):
        for name in args.scenario or SCENARIOS:
            directory = out / f"{name}-{run}"
            row = run_scenario(name, directory, args.ota_images and args.ota_images.resolve())
            report["profiles"].append(dict(row, run=run, directory=directory.name))
            (out / "runtime.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"{name} ({run}): {row['status']}, {'partial' if row['partial'] else 'samples captured'}", flush=True)
    return int(any(row["status"] != "passed" or row["partial"] for row in report["profiles"]))


def device(args):
    if args.seconds < 1:
        sys.exit("--seconds must be positive")
    password = os.environ.get("GATEWAY_PASSWORD")
    if password is None:
        sys.exit("set GATEWAY_PASSWORD before running the sampler")
    capture = sample_rest(args.url, args.user, password, args.seconds,
                          cafile=args.cafile)
    summary = rest_summary(capture)
    partial = bool(summary["errors"] or len(summary["boots"]) != 1)
    args.output.write_text(json.dumps(dict(platform="hardware", partial=partial, **summary, raw=capture),
                                     indent=2) + "\n")
    return int(partial)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    sim = commands.add_parser("qemu")
    sim.add_argument("--output", type=Path, required=True)
    sim.add_argument("--qemu-revision", required=True)
    sim.add_argument("--ota-images", type=Path)
    sim.add_argument("--runs", type=int, choices=range(1, 6), default=2)
    sim.add_argument("--scenario", choices=SCENARIOS, action="append")
    sim.set_defaults(func=qemu)
    hw = commands.add_parser("hardware")
    hw.add_argument("url")
    hw.add_argument("--user", required=True)
    hw.add_argument("--cafile")
    hw.add_argument("--seconds", type=int, default=IDLE_SECONDS)
    hw.add_argument("--output", type=Path, required=True)
    hw.set_defaults(func=device)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
