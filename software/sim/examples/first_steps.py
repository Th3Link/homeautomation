"""Run with ./sim -f tests/sim.toml run examples/first_steps.py."""

sim.wait_ready()

taster = sim["taster"]
taster.press(1, "double")
double_click = sim.expect_can("ButtonEvent", chip=taster, data=[1, 4, 0, 0])
print(f"{taster.name}: {double_click.text}")

relais = sim["relais"]
relais.press(5)
single_click = sim.expect_can("ButtonEvent", chip=relais, data=[5, 3, 0, 0])
print(f"{relais.name}: {single_click.text}")
