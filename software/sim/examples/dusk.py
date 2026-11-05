"""Evening in the hall: the light fades, someone walks past, the bathroom
gets humid. Run against sim.toml: ./sim run examples/dusk.py"""

import time

flur, bad = sim["taster-flur"], sim["taster-bad"]

print("dusk: 400 lx down to 5 lx over 30 s")
flur.veml7700.set(lux=400)
time.sleep(3)
flur.veml7700.ramp("lux", 5, secs=30, steps=10)
sim.expect_can("AmbientLightSensor", chip=flur, value=5, tolerance=1, timeout=40)
print("  the node reported it")

print("someone walks through the hall")
flur.motion(20)
sim.expect_can("PirSensor", chip=flur, data=[8, 1], timeout=5)
print("  PIR reported")

print("a shower upstairs: humidity 55 % -> 90 %")
bad.bme680.set(humidity=55)
bad.bme680.ramp("humidity", 90, secs=20, steps=5)
frame = sim.expect_can("HumiditySensor", chip=bad, value=90, timeout=45)
print(f"  the gateway got {frame.value:g} %")

time.sleep(1)
print("what the two nodes reported meanwhile:")
for e in sim.events(since=0):
    if e["kind"] == "can" and e["chip"] in (flur.name, bad.name) and e.get("value") is not None:
        print("  ", time.strftime("%H:%M:%S", time.localtime(e["t"])), e["chip"], e["text"])
