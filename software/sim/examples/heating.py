"""The buffer tank on relais-og cools down; watch the node and the gateway.
./sim run examples/heating.py [minutes]"""

import sys

minutes = float(sys.argv[1]) if len(sys.argv) > 1 else 2
tank = sim["relais-og"].ds18b20_0

print(f"tank 60 °C -> 35 °C over {minutes:g} min")
tank.set(temperature=60)
tank.ramp("temperature", 35, secs=minutes * 60, steps=int(minutes * 6))
serial = tank.rom[2:14]  # the ROM code without family byte and CRC
frame = sim.expect_can("TemperatureSensor", chip="relais-og", sensor=serial, value=35, timeout=60)
print(f"  reported: {frame.value:g} °C from {frame.sensor}")
