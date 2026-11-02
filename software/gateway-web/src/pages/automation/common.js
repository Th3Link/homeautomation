// Pieces the automation pages share: devices by name, times, conditions,
// the operations per output kind, and an editor dialog.
import { useEffect, useState } from 'preact/hooks';
import * as api from '../../api.js';
import { html, Field } from '../../ui.js';

// ---- devices -------------------------------------------------------------

// The devices on the bus, loaded once per page.
let devicesPromise;
export function useDevices() {
  const [devices, setDevices] = useState([]);
  useEffect(() => {
    devicesPromise = devicesPromise || api.devices.list().catch(() => ((devicesPromise = undefined), []));
    devicesPromise.then(setDevices);
  }, []);
  return devices;
}

export const refKey = (ref) => (ref ? `${ref.type}:${ref.id}` : '');
const toRef = (key) => {
  const [type, id] = key.split(':').map(Number);
  return { type, id };
};

// "kitchen" for a device with a name, else "type 3 / 0x06".
export function deviceName(devices, ref) {
  if (!ref) return '—';
  const d = devices.find((d) => d.device_type === ref.type && d.device_id === ref.id);
  if (d && d.name) return d.name;
  return `${d ? d.type_name : 'type ' + ref.type} / 0x${ref.id.toString(16).padStart(2, '0')}`;
}

// A device: from the list, or typed as type and id for one that isn't on the
// bus right now. `types` narrows the list to some device types.
export function DeviceSelect({ value, onChange, devices, types }) {
  const known = devices.filter((d) => !types || types.includes(d.device_type));
  const key = refKey(value);
  const listed = known.some((d) => refKey({ type: d.device_type, id: d.device_id }) === key);
  return html`<div class="device-select">
    <select value=${listed ? key : value ? 'manual' : ''} onChange=${(e) => {
      const v = e.target.value;
      if (v === 'manual') onChange(value || { type: (types && types[0]) || 0, id: 1 });
      else if (v) onChange(toRef(v));
    }}>
      <option value="" disabled>Choose a device…</option>
      ${known.map(
        (d) => html`<option value=${refKey({ type: d.device_type, id: d.device_id })}>
          ${deviceName(devices, { type: d.device_type, id: d.device_id })} (${d.type_name})
        </option>`,
      )}
      <option value="manual">Other: type and id…</option>
    </select>
    ${value && !listed &&
    html`<span class="pair">
      <input type="number" min="0" max="63" title="Device type" value=${value.type} onInput=${(e) => onChange({ ...value, type: +e.target.value })} />
      <input type="number" min="1" max="255" title="Device id" value=${value.id} onInput=${(e) => onChange({ ...value, id: +e.target.value })} />
    </span>`}
  </div>`;
}

// ---- times and conditions ------------------------------------------------

export const SUN_EVENTS = [
  ['astronomical_dawn', 'Astronomical dawn'],
  ['nautical_dawn', 'Nautical dawn'],
  ['civil_dawn', 'Civil dawn'],
  ['sunrise', 'Sunrise'],
  ['noon', 'Noon'],
  ['sunset', 'Sunset'],
  ['civil_dusk', 'Civil dusk'],
  ['nautical_dusk', 'Nautical dusk'],
  ['astronomical_dusk', 'Astronomical dusk'],
];
const sunLabel = (e) => (SUN_EVENTS.find(([k]) => k === e) || [e, e])[1];

const pad = (n) => String(n).padStart(2, '0');

export function timeText(t) {
  if (!t) return '';
  if (t.clock) return `${pad(t.clock.hour)}:${pad(t.clock.minute)}`;
  const off = t.sun.offset_min || 0;
  return sunLabel(t.sun.event) + (off ? ` ${off > 0 ? '+' : '−'} ${Math.abs(off)} min` : '');
}

// A time of day: on the clock or relative to the sun.
export function TimeEdit({ value, onChange }) {
  const sun = !!value.sun;
  return html`<span class="time-edit">
    <select value=${sun ? 'sun' : 'clock'} onChange=${(e) =>
      onChange(e.target.value === 'sun' ? { sun: { event: 'sunset', offset_min: 0 } } : { clock: { hour: 22, minute: 0 } })}>
      <option value="clock">Clock</option>
      <option value="sun">Sun</option>
    </select>
    ${sun
      ? html`<select value=${value.sun.event} onChange=${(e) => onChange({ sun: { ...value.sun, event: e.target.value } })}>
            ${SUN_EVENTS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
          </select>
          <input type="number" class="short" min="-720" max="720" title="Offset in minutes (negative: before)" value=${value.sun.offset_min || 0}
            onInput=${(e) => onChange({ sun: { ...value.sun, offset_min: +e.target.value } })} /><small>min</small>`
      : html`<input type="time" value=${`${pad(value.clock.hour)}:${pad(value.clock.minute)}`} onInput=${(e) => {
          const [h, m] = e.target.value.split(':').map(Number);
          if (!isNaN(h)) onChange({ clock: { hour: h, minute: m || 0 } });
        }} />`}
  </span>`;
}

export const CONDITIONS = [
  ['dark', 'Darker than'],
  ['bright', 'At least as bright as'],
  ['between', 'Time between'],
  ['sun_below', 'Sun lower than'],
  ['sun_above', 'Sun at least'],
];

const newCondition = (kind, devices) => {
  const sensor = { type: 0, id: 1 };
  switch (kind) {
    case 'dark':
    case 'bright':
      return { [kind]: { sensor, lux: 40 } };
    case 'between':
      return { between: { from: { clock: { hour: 22, minute: 0 } }, to: { clock: { hour: 6, minute: 0 } } } };
    default:
      return { [kind]: { degrees: -6 } };
  }
};

export const conditionKind = (c) => Object.keys(c)[0];

export function conditionText(c, devices) {
  const kind = conditionKind(c);
  const v = c[kind];
  switch (kind) {
    case 'dark':
      return `darker than ${v.lux} lx at ${deviceName(devices, v.sensor)}`;
    case 'bright':
      return `at least ${v.lux} lx at ${deviceName(devices, v.sensor)}`;
    case 'between':
      return `between ${timeText(v.from)} and ${timeText(v.to)}`;
    case 'sun_below':
      return `sun below ${v.degrees}°`;
    case 'sun_above':
      return `sun at least ${v.degrees}°`;
  }
  return kind;
}

// Devices with a brightness sensor, for the lux conditions.
let luxPromise;
export function useLuxSensors() {
  const [keys, setKeys] = useState([]);
  useEffect(() => {
    luxPromise =
      luxPromise ||
      api.sensors
        .list()
        .then((all) => all.filter((s) => s.kind === 'brightness'))
        .catch(() => ((luxPromise = undefined), []));
    luxPromise.then(setKeys);
  }, []);
  return keys;
}

export function ConditionEdit({ value, onChange, devices }) {
  const kind = conditionKind(value);
  const v = value[kind];
  const lux = useLuxSensors();
  // Devices with a light sensor first; any device can be picked.
  const sensorDevices = devices.filter((d) => lux.some((s) => s.device_key.toLowerCase() === d.key.toLowerCase()));
  const set = (patch) => onChange({ [kind]: { ...v, ...patch } });
  return html`<span class="condition">
    <select value=${kind} onChange=${(e) => onChange(newCondition(e.target.value, devices))}>
      ${CONDITIONS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
    </select>
    ${(kind === 'dark' || kind === 'bright') &&
    html`<input type="number" class="short" min="0" value=${v.lux} onInput=${(e) => set({ lux: +e.target.value })} /><small>lx at</small>
      <${DeviceSelect} value=${v.sensor.type ? v.sensor : null} devices=${sensorDevices.length ? sensorDevices : devices} onChange=${(sensor) => set({ sensor })} />`}
    ${kind === 'between' &&
    html`<${TimeEdit} value=${v.from} onChange=${(from) => set({ from })} /><small>and</small><${TimeEdit} value=${v.to} onChange=${(to) => set({ to })} />`}
    ${(kind === 'sun_below' || kind === 'sun_above') &&
    html`<input type="number" class="short" min="-90" max="90" value=${v.degrees} onInput=${(e) => set({ degrees: +e.target.value })} /><small>° (−6: civil twilight ends)</small>`}
  </span>`;
}

// A list of up to `max` items with add and remove buttons.
export function ListEdit({ items, onChange, max, make, render, addLabel }) {
  return html`<div class="list-edit">
    ${items.map(
      (item, i) => html`<div class="list-row" key=${i}>
        <div class="list-body">${render(item, (next) => onChange(items.map((x, j) => (j === i ? next : x))), i)}</div>
        <button class="icon" title="Remove" onClick=${() => onChange(items.filter((_, j) => j !== i))}>×</button>
      </div>`,
    )}
    ${items.length < max && html`<button onClick=${() => onChange([...items, make()])}>+ ${addLabel}</button>`}
  </div>`;
}

// ---- outputs -------------------------------------------------------------

export const OUTPUT_KINDS = [
  ['light', 'Light (levels)'],
  ['dimmer', 'Dimmer (0–255)'],
  ['relay', 'Relay'],
  ['shutter', 'Roller shutter'],
  ['group', 'Group'],
];

// What can be done to an output of each kind: [op, label, takes a value].
export const OPS = {
  light: [['toggle', 'Toggle'], ['on', 'On (last level)'], ['off', 'Off'], ['set', 'Level', true], ['max', 'Brightest'], ['inc', 'Brighter'], ['dec', 'Darker']],
  dimmer: [['toggle', 'Toggle'], ['on', 'On (last value)'], ['off', 'Off'], ['set', 'Value', true], ['max', 'Full'], ['inc', 'Brighter'], ['dec', 'Darker']],
  relay: [['toggle', 'Toggle'], ['on', 'On'], ['off', 'Off'], ['on_for', 'On for (ms)', true]],
  shutter: [['up', 'Up / stop'], ['down', 'Down / stop'], ['up_hold', 'Up (while held)'], ['down_hold', 'Down (while held)'], ['stop', 'Stop']],
  group: [['toggle', 'Toggle'], ['on', 'On'], ['off', 'Off'], ['up', 'Up / stop'], ['down', 'Down / stop'], ['stop', 'Stop']],
};

// {"set": 4} → ["set", 4]; "on" → ["on", undefined].
export const opParts = (op) => (typeof op === 'string' ? [op, undefined] : [Object.keys(op)[0], Object.values(op)[0]]);
export const makeOp = (name, value) => (value === undefined ? name : { [name]: value });

export function opText(op, kind) {
  const [name, value] = opParts(op);
  const found = (OPS[kind] || Object.values(OPS).flat()).find(([k]) => k === name);
  const label = found ? found[1].replace(/ \(.*\)$/, '') : name;
  return value === undefined ? label : `${label} ${value}`;
}

export function OpSelect({ value, onChange, kind }) {
  const ops = OPS[kind] || OPS.light;
  const [name, arg] = opParts(value);
  const takes = (n) => (ops.find(([k]) => k === n) || [])[2];
  return html`<span class="op">
    <select value=${name} onChange=${(e) => onChange(makeOp(e.target.value, takes(e.target.value) ? (e.target.value === 'on_for' ? 60000 : 1) : undefined))}>
      ${ops.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
    </select>
    ${takes(name) && html`<input type="number" class="short" min="0" value=${arg} onInput=${(e) => onChange(makeOp(name, +e.target.value))} />`}
  </span>`;
}

// ---- layout --------------------------------------------------------------

// An editor shown as a card in place of the list, with its own error line.
export function Editor({ title, children, onSave, onCancel, error }) {
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      await onSave();
    } finally {
      setBusy(false);
    }
  };
  return html`<section class="card editor">
    <header><h2>${title}</h2></header>
    ${children}
    ${error && html`<p class="error">${error}</p>`}
    <div class="buttons">
      <button class="primary" disabled=${busy} onClick=${save}>Save</button>
      <button onClick=${onCancel}>Cancel</button>
    </div>
  </section>`;
}

// The message of a refused save, with the field it is about.
export const errorText = (e) => (e.field ? `${e.field}: ${e.message}` : e.message);

export { Field };
