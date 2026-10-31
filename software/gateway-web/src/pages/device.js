import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { FirmwareUpload } from '../firmware-upload.js';
import { SensorTable, ROLES, roleLabel, useSensors } from './sensors.js';
import { html, useLoad, run, notify, ago, Section, Loading, Field, Button } from '../ui.js';

// Device types as the CAN protocol numbers them.
const HAS_RELAYS = [2, 5, 8, 9];
const HAS_ROLLERSHUTTERS = [7];
const HAS_LAMPS = [3];
// A light switch (type 4) drives lamps only through PCA9685 boards on its
// extension (extension mode 3, PWM), banks 1-16.
const EXTENSION_PWM = 3;
const IS_HEATING = [9];

const RELAIS_MODES = ['OFF', 'RELAIS', 'SWROLLERSHUTTER', 'HWROLLERSHUTTER'];
const EXTENSION_MODES = ['OFF', 'BUTTONS', 'RELAIS', 'SWROLLERSHUTTER', 'HWROLLERSHUTTER', 'PWM', 'SENSORS', 'LEGACY_SENSORS'];

export function Device({ device }) {
  const { data, error, reload } = useLoad(() => api.devices.get(device), 5000);
  if (!data) return html`<${Loading} error=${error} />`;

  return html`
    <${Overview} d=${data} reload=${reload} />
    ${HAS_RELAYS.includes(data.device_type) && html`<${Relays} d=${data} kind="relay" />`}
    ${HAS_ROLLERSHUTTERS.includes(data.device_type) && html`<${Relays} d=${data} kind="rollershutter" />`}
    ${HAS_LAMPS.includes(data.device_type) && html`<${Lamps} d=${data} />`}
    ${data.device_type === 4 && data.extension_mode === EXTENSION_PWM && html`<${Lamps} d=${data} firstBank=${1} />`}
    ${IS_HEATING.includes(data.device_type) && html`<${Heating} d=${data} />`}
    ${IS_HEATING.includes(data.device_type) && html`<${SensorAssignment} d=${data} />`}
    ${IS_HEATING.includes(data.device_type) && html`<${HeatingManual} d=${data} />`}
    <${Section} title="Sensors"><${SensorTable} device=${data.name || data.key} /><//>
    <${Settings} d=${data} reload=${reload} />
    <${DeviceFirmware} d=${data} />`;
}

// Sends a firmware image over the CAN bus: to this device, or one after the
// other to every device of its type.
function DeviceFirmware({ d }) {
  const id = d.name || d.key;
  const { data: devices } = useLoad(api.devices.list);
  const others = (devices || [])
    .filter((o) => o.device_type === d.device_type && o.key !== d.key)
    .map((o) => ({ name: o.name || o.key, target: 'devices/' + encodeURIComponent(o.name || o.key) }));
  return html`
    <${Section} title="Firmware">
      <p class="muted">Sends a firmware image to this device over the CAN bus: in blocks that the device acknowledges and the gateway repeats if the bus lost frames, so it takes a few minutes and does not depend on timing. The device restarts into the new image and keeps it once it was heard on the bus; if it does not come back, it returns to the old one.</p>
      <${FirmwareUpload} target=${'devices/' + encodeURIComponent(id)} label="Send to this device" others=${others} />
    <//>`;
}

function Overview({ d, reload }) {
  const id = d.name || d.key;
  const act = (action, text) => async () => {
    await run(() => api.devices.action(id, action), text);
    setTimeout(reload, 1500);
  };
  return html`
    <${Section} title=${id}
      aside=${html`<span class="buttons">
        <${Button} onClick=${act('ping', 'Ping sent')}>Ping<//>
        <${Button} onClick=${act('refresh', 'Refresh requested')}>Refresh<//>
        <${Button} kind="danger" onClick=${act('restart', 'Restart sent')}>Restart<//>
      </span>`}>
      <dl class="facts">
        <div><dt>Type</dt><dd>${d.type_name} (${d.device_type})</dd></div>
        <div><dt>Key</dt><dd>${d.key}</dd></div>
        <div><dt>Device ID</dt><dd>${d.device_id}</dd></div>
        <div><dt>Version</dt><dd>${d.version}</dd></div>
        <div><dt>Last seen</dt><dd>${ago(d.last_seen_minutes_ago)}</dd></div>
        <div><dt>Hardware rev.</dt><dd>${d.hwrev}</dd></div>
        <div><dt>UID</dt><dd>${d.uid0} / ${d.uid1}</dd></div>
        <div><dt>State / error</dt><dd>${d.state} / ${d.error}</dd></div>
      </dl>
    <//>`;
}

function Relays({ d, kind }) {
  const id = d.name || d.key;
  const [number, setNumber] = useState(1);
  const [duration, setDuration] = useState(0);
  const shutter = kind === 'rollershutter';
  const send = (state) => () =>
    run(() => api.devices[kind](id, number, state, Math.round(duration * 1000)), `${shutter ? 'Rollershutter' : 'Relay'} ${number}: ${state}`);
  return html`
    <${Section} title=${shutter ? 'Rollershutter' : 'Relays'}>
      ${d.device_type === 9 && html`<p class="muted">The heating controller drives relays 0-10 itself and ignores commands for them; only relay 11 can be switched here. For relays 6-10 see manual mode below.</p>`}
      <div class="row">
        <${Field} label="Number"><input type="number" min="1" max="16" value=${number} onInput=${(e) => setNumber(+e.target.value)} /><//>
        <${Field} label="Switch back after (s)" hint="0 = stay"><input type="number" min="0" step="0.1" value=${duration} onInput=${(e) => setDuration(+e.target.value)} /><//>
      </div>
      <div class="buttons">
        ${shutter
          ? html`<${Button} onClick=${send('up')}>Up<//><${Button} onClick=${send('down')}>Down<//><${Button} onClick=${send('off')}>Stop<//>`
          : html`<${Button} kind="primary" onClick=${send('on')}>On<//><${Button} onClick=${send('off')}>Off<//>`}
      </div>
    <//>`;
}

function Lamps({ d, firstBank = 0 }) {
  const id = d.name || d.key;
  const [bank, setBank] = useState(firstBank);
  const [value, setValue] = useState(255);
  const [lamps, setLamps] = useState(Array(24).fill(false));
  // Shown most significant bit first, like the old UI did.
  const bitmask = lamps.reduce((m, on, i) => (on ? m | (1 << (23 - i)) : m), 0);
  return html`
    <${Section} title="Lamps">
      <p class="muted">The numbers are the bits of the bank's lamp mask.</p>
      <div class="row">
        <${Field} label="Bank"><input type="number" min=${firstBank} max="255" value=${bank} onInput=${(e) => setBank(+e.target.value)} /><//>
        <${Field} label="Value (0-255)"><input type="number" min="0" max="255" value=${value} onInput=${(e) => setValue(+e.target.value)} /><//>
      </div>
      <div class="lamps">
        ${lamps.map(
          (on, i) => html`<label key=${i} class=${'lamp' + (on ? ' on' : '')}>
            <input type="checkbox" checked=${on} onChange=${() => setLamps(lamps.map((l, j) => (j === i ? !l : l)))} />${23 - i}
          </label>`,
        )}
      </div>
      <${Button} kind="primary" onClick=${() => run(() => api.devices.lamps(id, bank, value, bitmask), 'Lamps set')}>Send<//>
    <//>`;
}

function Heating({ d }) {
  const id = d.name || d.key;
  const [flow, setFlow] = useState(45);
  const [tank, setTank] = useState(false);
  const [power, setPower] = useState(0);
  const [rod, setRod] = useState(false);
  const [stage, setStage] = useState(0);
  const [tankC, setTankC] = useState(55);
  return html`
    <${Section} title="Heating control">
      <p class="muted">The controller switches the rod off again when these stop coming (after 30 s).</p>
      <h3>Target</h3>
      <div class="row">
        <${Field} label="Flow (°C)"><input type="number" step="0.5" value=${flow} onInput=${(e) => setFlow(+e.target.value)} /><//>
        <label class="check"><input type="checkbox" checked=${tank} onChange=${(e) => setTank(e.target.checked)} /> from the tank</label>
        <${Button} onClick=${() => run(() => api.devices.heating(id, 'target', { flow_c: flow, tank }), 'Target sent')}>Send<//>
      </div>
      <h3>Energy</h3>
      <div class="row">
        <${Field} label="Power (W)" hint="negative while importing"><input type="number" step="100" value=${power} onInput=${(e) => setPower(+e.target.value)} /><//>
        <label class="check"><input type="checkbox" checked=${rod} onChange=${(e) => setRod(e.target.checked)} /> rod released</label>
        <${Button} onClick=${() => run(() => api.devices.heating(id, 'energy', { power_w: power, rod_enabled: rod }), 'Energy sent')}>Send<//>
      </div>
      <h3>Emergency</h3>
      <div class="row">
        <${Field} label="Stage (0-3)"><input type="number" min="0" max="3" value=${stage} onInput=${(e) => setStage(+e.target.value)} /><//>
        <${Field} label="Tank (°C)"><input type="number" step="0.5" value=${tankC} onInput=${(e) => setTankC(+e.target.value)} /><//>
        <${Button} kind="danger" onClick=${() => run(() => api.devices.heating(id, 'emergency', { stage, tank_c: tankC }), 'Emergency sent')}>Send<//>
      </div>
    <//>`;
}

// The relays 6-10 of the controller, the ones manual mode hands over.
const MANUAL_RELAYS = [
  [6, 'Flow valve'],
  [7, 'Return valve'],
  [8, 'Mixer towards hot'],
  [9, 'Mixer towards cold'],
  [10, 'Circuit pump'],
];
// Which sensor has which role in the heating controller. The sensors are named
// below in the sensor table; the slots they are called by are the node's own
// and temporary, the gateway handles that.
function SensorAssignment({ d }) {
  const id = d.name || d.key;
  const { data, error, reload, ask } = useSensors(id);
  if (!data) return html`<${Section} title="Sensor assignment"><${Loading} error=${error} /><//>`;

  const temperatures = data.filter((s) => s.kind === 'temperature');
  const holder = (role) => temperatures.find((s) => s.role === role);
  const label = (s) => (s.name ? `${s.name} (${s.sensor_id})` : s.sensor_id);
  const known = temperatures.every((s) => s.slot !== null);

  const assign = (role) => async (e) => {
    const chosen = e.target.value;
    let ok;
    if (chosen === '') {
      const current = holder(role);
      if (!current) return;
      ok = await run(() => api.sensors.update(id, current.slot, { role: 'none' }), `${roleLabel(role)} cleared`);
    } else {
      const sensor = temperatures.find((s) => s.sensor_id === chosen);
      ok = await run(() => api.sensors.update(id, sensor.slot, { role }), `${label(sensor)} is ${roleLabel(role)} now`);
    }
    if (ok) setTimeout(reload, 1500);
  };

  return html`
    <${Section} title="Sensor assignment" aside=${html`<${Button} onClick=${ask}>Reload from the device<//>`}>
      <p class="muted">Which temperature sensor is the tank at each height, and which one measures the flow. The heating needs them to run the rod and the circuit.</p>
      ${!known && html`<p class="muted">Some sensors have not reported their slot yet; "Reload from the device" asks for it.</p>`}
      <div class="roles">
        ${ROLES.filter((r) => r !== 'none').map(
          (role) => html`<label key=${role} class="field">
            <span>${roleLabel(role)}</span>
            <select value=${(holder(role) || {}).sensor_id || ''} onChange=${assign(role)} disabled=${!known}>
              <option value="">(none)</option>
              ${temperatures.map((s) => html`<option value=${s.sensor_id}>${label(s)}</option>`)}
            </select>
          </label>`,
        )}
      </div>
    <//>`;
}

function HeatingManual({ d }) {
  const id = d.name || d.key;
  // What the controller says: it reports its manual mode every few seconds.
  const { data, error, reload } = useLoad(() => api.devices.heatingManualState(id), 3000);
  const manual = !!(data && data.enabled);
  const on = (relay) => !!(data && data.relays.includes(relay));

  const toggleMode = async () => {
    const enable = !manual;
    if (await run(() => api.devices.heatingManual(id, enable), enable ? 'Manual mode on: valves, mixer and pump are off' : 'Manual mode off: the controller has them back')) {
      setTimeout(reload, 800);
    }
  };

  const toggle = (relay) => async () => {
    if (await run(() => api.devices.heatingManual(id, true, relay, !on(relay)), `Relay ${relay} ${on(relay) ? 'off' : 'on'}`)) {
      setTimeout(reload, 800);
    }
  };

  // The two mixer directions must never be on together.
  const blocked = (relay) => (relay === 8 && on(9)) || (relay === 9 && on(8));

  return html`
    <${Section} title="Manual mode"
      aside=${html`<${Button} kind=${manual ? 'danger' : ''} disabled=${!data} onClick=${toggleMode}>${manual ? 'End manual mode' : 'Start manual mode'}<//>`}>
      <p class="muted">For commissioning, venting and the like: the controller leaves valves, mixer and pump alone (they all go off when this starts) and you switch them one by one. It stays on until you end it, however long that takes — only a restart of the controller ends it too. While it is on, commands for the heating (from MQTT, too) are dropped, so the heating rod goes off. The rod is never switchable here.</p>
      ${error && html`<p class="error">The controller does not answer: ${error.message}</p>`}
      ${manual && html`<p class="badge warn">Manual mode is on</p>`}
      <div class="manual">
        ${MANUAL_RELAYS.map(
          ([relay, label]) => html`<button key=${relay} class=${on(relay) ? 'primary' : ''} disabled=${!manual || blocked(relay)} onClick=${toggle(relay)}>
            <span>${label}</span><small>relay ${relay}: ${on(relay) ? 'on' : 'off'}</small>
          </button>`,
        )}
      </div>
    <//>`;
}

function Settings({ d, reload }) {
  const id = d.name || d.key;
  const [form, setForm] = useState({ custom_string: d.name, hwrev: d.hwrev, relais_mode: '', extension_mode: '' });
  const [silent, setSilent] = useState(false);
  const set = (key) => (e) => setForm({ ...form, [key]: e.target.value });

  const save = async () => {
    const changes = {};
    if (form.custom_string !== d.name) changes.custom_string = form.custom_string;
    if (+form.hwrev !== d.hwrev) changes.hwrev = +form.hwrev;
    if (form.relais_mode) changes.relais_mode = form.relais_mode;
    if (form.extension_mode) changes.extension_mode = form.extension_mode;
    if (Object.keys(changes).length === 0) return notify('Nothing changed');
    if (await run(() => api.devices.update(id, changes), 'Settings sent')) setTimeout(reload, 1500);
  };

  return html`
    <${Section} title="Settings">
      <div class="row">
        <${Field} label="Name" hint="up to 8 characters"><input maxlength="8" value=${form.custom_string} onInput=${set('custom_string')} /><//>
        <${Field} label="Hardware revision"><input type="number" min="0" max="255" value=${form.hwrev} onInput=${set('hwrev')} /><//>
        <${Field} label="Relay mode"><select value=${form.relais_mode} onChange=${set('relais_mode')}>
          <option value="">(unchanged)</option>${RELAIS_MODES.map((m) => html`<option>${m}</option>`)}
        </select><//>
        <${Field} label="Extension mode"><select value=${form.extension_mode} onChange=${set('extension_mode')}>
          <option value="">(unchanged)</option>${EXTENSION_MODES.map((m) => html`<option>${m}</option>`)}
        </select><//>
      </div>
      <div class="buttons">
        <${Button} kind="primary" onClick=${save}>Save<//>
        <${Button} onClick=${async () => {
          await run(() => api.devices.silence(id, !silent), silent ? 'Device may talk again' : 'Device silenced');
          setSilent(!silent);
        }}>${silent ? 'End silence' : 'Silence'}<//>
        <${Button} onClick=${() => run(() => api.devices.action(id, 'legacy-mode'), 'Update mode requested')}>Legacy update mode<//>
      </div>
    <//>`;
}
