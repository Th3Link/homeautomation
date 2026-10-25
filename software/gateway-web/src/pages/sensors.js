import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { Link } from '../router.js';
import { html, useLoad, run, ago, Section, Loading, Button } from '../ui.js';

const KINDS = { temperature: 'Temperature', humidity: 'Humidity', pressure: 'Pressure', co2: 'CO₂', voc: 'VOC', air_quality: 'Air quality' };
export const ROLES = ['none', 'tank0', 'tank1', 'tank2', 'tank3', 'tank4', 'tank5', 'tank6', 'flow'];
const ROLE_LABELS = { none: '–', flow: 'Flow', ...Object.fromEntries([0, 1, 2, 3, 4, 5, 6].map((n) => [`tank${n}`, `Tank ${n}${n === 0 ? ' (top)' : n === 6 ? ' (bottom)' : ''}`])) };
export const roleLabel = (role) => ROLE_LABELS[role] || role;

// The sensors (of one device if `device` is given) and a way to ask the
// nodes for their slots, names and roles.
export function useSensors(device, every = 5000) {
  const state = useLoad(() => api.sensors.list(device), every);
  const devices = [...new Set((state.data || []).map((s) => s.device))];

  const ask = async () => {
    for (const name of device ? [device] : devices) await run(() => api.sensors.refresh(name));
    setTimeout(state.reload, 1500);
  };
  return { ...state, ask };
}

// The node reports slot, name and role once it was asked; the gateway asks by
// itself the first time it sees a sensor, so this is only empty briefly.
function Row({ s, device, reload }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(s.name || '');
  const [role, setRole] = useState(s.role || 'none');

  const save = async () => {
    const changes = {};
    if (name !== (s.name || '')) changes.name = name;
    if (role !== (s.role || 'none')) changes.role = role;
    if (Object.keys(changes).length === 0) return setEditing(false);
    if (await run(() => api.sensors.update(s.device, s.slot, changes), 'Sent to the device')) {
      setEditing(false);
      setTimeout(reload, 1500);
    }
  };

  return html`<tr>
    ${!device && html`<td><${Link} href=${'/devices/' + encodeURIComponent(s.device)}>${s.device}<//></td>`}
    <td>${editing
      ? html`<input class="inline" maxlength="7" placeholder="name" value=${name} onInput=${(e) => setName(e.target.value)} />`
      : s.name || html`<span class="muted">–</span>`}</td>
    <td>${KINDS[s.kind] || s.kind}</td>
    <td>${editing
      ? html`<select value=${role} onChange=${(e) => setRole(e.target.value)}>${ROLES.map((r) => html`<option value=${r}>${roleLabel(r)}</option>`)}</select>`
      : s.role && s.role !== 'none' ? roleLabel(s.role) : html`<span class="muted">–</span>`}</td>
    <td><code>${s.sensor_id}</code>${s.slot !== null && html` <small class="muted">slot ${s.slot}</small>`}</td>
    <td>${s.lost ? html`<span class="badge bad">lost</span>` : `${s.kind === 'temperature' ? s.value.toFixed(2) : s.value} ${s.unit}`}</td>
    <td>${s.age_s < 60 ? `${s.age_s} s` : ago(Math.floor(s.age_s / 60))}</td>
    <td class="actions">${s.slot === null
      ? ''
      : editing
        ? html`<${Button} kind="primary" onClick=${save}>Save<//> <button onClick=${() => setEditing(false)}>Cancel</button>`
        : html`<button onClick=${() => setEditing(true)}>Edit</button>`}</td>
  </tr>`;
}

// The table of sensors; of one device if `device` is given, else of all.
export function SensorTable({ device }) {
  const { data, error, reload, ask } = useSensors(device);
  if (!data) return html`<${Loading} error=${error} />`;
  if (data.length === 0) return html`<p class="muted">No sensor reading seen yet.</p>`;
  const unknown = data.some((s) => s.slot === null);
  return html`
    ${unknown && html`<p class="muted">The names and roles of some sensors are not known yet. <${Button} onClick=${ask}>Ask the device<//></p>`}
    <div class="table-wrap"><table>
      <thead><tr>${!device && html`<th>Device</th>`}<th>Name</th><th>Kind</th><th>Role</th><th>Sensor</th><th>Value</th><th>Seen</th><th></th></tr></thead>
      <tbody>
        ${data.map((s) => html`<${Row} key=${s.device_key + s.kind + s.sensor_id} s=${s} device=${device} reload=${reload} />`)}
      </tbody>
    </table></div>`;
}

export function Sensors() {
  const { ask } = useSensors(undefined, 0);
  return html`<${Section} title="Sensors" aside=${html`<${Button} onClick=${ask}>Reload names and roles<//>`}>
    <${SensorTable} />
  <//>`;
}
