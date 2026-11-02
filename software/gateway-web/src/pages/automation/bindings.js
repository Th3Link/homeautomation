// Button bindings: a button event on a switch and up to four actions.
import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from '../../api.js';
import { html, run, notify, Section, Loading, Field } from '../../ui.js';
import { DeviceSelect, deviceName, Editor, errorText, ListEdit, OpSelect, opText, useDevices } from './common.js';

export const EVENTS = [
  ['single', 'Pressed once'],
  ['double', 'Pressed twice'],
  ['triple', 'Pressed three times'],
  ['hold', 'Held (every 0.5 s)'],
  ['released', 'Let go after holding'],
];
const eventLabel = (e) => (EVENTS.find(([k]) => k === e) || [e, e])[1];
const MAX_ACTIONS = 4;

function actionText(a, devices, kinds) {
  if (a.output) return `${a.output.name}: ${opText(a.output.op, kinds[a.output.name])}`;
  if (a.automation) return `${a.automation.name}: ${a.automation.switch}`;
  if (a.nightlight) return `night light of ${deviceName(devices, a.nightlight.device)}: ${a.nightlight.value}`;
  return '?';
}

function ActionEdit({ value, onChange, devices, outputs, automations }) {
  const type = Object.keys(value)[0];
  const v = value[type];
  const kinds = Object.fromEntries(outputs.map((o) => [o.name, o.kind]));
  const switchTo = (t) => {
    if (t === 'output') onChange({ output: { name: outputs[0] ? outputs[0].name : '', op: 'toggle' } });
    if (t === 'automation') onChange({ automation: { name: automations[0] || '', switch: 'toggle' } });
    if (t === 'nightlight') onChange({ nightlight: { device: { type: 4, id: 1 }, value: 0 } });
  };
  return html`<span class="action">
    <select value=${type} onChange=${(e) => switchTo(e.target.value)}>
      <option value="output">Output</option>
      <option value="automation">Motion light / schedule</option>
      <option value="nightlight">Night light</option>
    </select>
    ${type === 'output' &&
    html`<select value=${v.name} onChange=${(e) => onChange({ output: { name: e.target.value, op: 'toggle' } })}>
        ${!outputs.some((o) => o.name === v.name) && html`<option value=${v.name}>${v.name || 'Choose…'}</option>`}
        ${outputs.map((o) => html`<option value=${o.name}>${o.name}</option>`)}
      </select>
      <${OpSelect} value=${v.op} kind=${kinds[v.name]} onChange=${(op) => onChange({ output: { ...v, op } })} />`}
    ${type === 'automation' &&
    html`<select value=${v.name} onChange=${(e) => onChange({ automation: { ...v, name: e.target.value } })}>
        ${!automations.includes(v.name) && html`<option value=${v.name}>${v.name || 'Choose…'}</option>`}
        ${automations.map((n) => html`<option value=${n}>${n}</option>`)}
      </select>
      <select value=${v.switch} onChange=${(e) => onChange({ automation: { ...v, switch: e.target.value } })}>
        <option value="toggle">Toggle</option>
        <option value="enable">Enable</option>
        <option value="disable">Disable</option>
      </select>`}
    ${type === 'nightlight' &&
    html`<${DeviceSelect} value=${v.device} devices=${devices} types=${[4]} onChange=${(device) => onChange({ nightlight: { ...v, device } })} />
      <input type="number" class="short" min="0" max="255" value=${v.value} onInput=${(e) => onChange({ nightlight: { ...v, value: +e.target.value } })} />`}
  </span>`;
}

// Waits for the next button event on the bus and hands it to `onLearn`.
function Learn({ onLearn }) {
  const [waiting, setWaiting] = useState(false);
  const stop = useRef(false);
  useEffect(() => () => (stop.current = true), []);
  const start = async () => {
    setWaiting(true);
    stop.current = false;
    const t0 = Date.now();
    const deadline = t0 + 30000;
    while (!stop.current && Date.now() < deadline) {
      try {
        const s = await api.automation.status();
        // Pressed after we started waiting (the age is in whole seconds).
        if (s.last_button && s.last_button.age_s + 1 <= (Date.now() - t0) / 1000) {
          onLearn(s.last_button.trigger);
          notify('Button learned');
          break;
        }
      } catch (e) {}
      await new Promise((r) => setTimeout(r, 700));
    }
    setWaiting(false);
  };
  return waiting
    ? html`<button class="learning" onClick=${() => (stop.current = true)}>Press the button now… (cancel)</button>`
    : html`<button onClick=${start}>Learn: press the button</button>`;
}

function BindingEditor({ slot, original, devices, outputs, automations, done }) {
  const [b, setB] = useState(
    original || { trigger: { device: null, button: 0, event: 'single' }, actions: [{ output: { name: outputs[0] ? outputs[0].name : '', op: 'toggle' } }] },
  );
  const [error, setError] = useState('');
  const t = b.trigger;
  const setTrigger = (patch) => setB({ ...b, trigger: { ...t, ...patch } });
  const save = async () => {
    if (!t.device) return setError('Choose the switch, or learn it');
    try {
      if (slot === undefined) await api.automation.bindings.add(b);
      else await api.automation.bindings.put(slot, b);
      notify('Binding saved');
      done(true);
    } catch (e) {
      setError(errorText(e));
    }
  };
  return html`<${Editor} title=${slot === undefined ? 'New button binding' : 'Button binding'} onSave=${save} onCancel=${() => done(false)} error=${error}>
    <div class="row">
      <${Field} label="Switch"><${DeviceSelect} value=${t.device} devices=${devices} onChange=${(device) => setTrigger({ device })} /><//>
      <${Field} label="Button" hint="0–7; 4–7 on an extension board">
        <input type="number" min="0" max="15" value=${t.button} onInput=${(e) => setTrigger({ button: +e.target.value })} />
      <//>
      <${Field} label="When">
        <select value=${t.event} onChange=${(e) => setTrigger({ event: e.target.value })}>
          ${EVENTS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
        </select>
      <//>
      <${Learn} onLearn=${(trigger) => setB((cur) => ({ ...cur, trigger }))} />
    </div>
    <h3>Does</h3>
    <${ListEdit} items=${b.actions} max=${MAX_ACTIONS} addLabel="action" onChange=${(actions) => setB({ ...b, actions })}
      make=${() => ({ output: { name: outputs[0] ? outputs[0].name : '', op: 'toggle' } })}
      render=${(a, set) => html`<${ActionEdit} value=${a} onChange=${set} devices=${devices} outputs=${outputs} automations=${automations} />`} />
  <//>`;
}

export function Bindings({ outputs, motion, schedules }) {
  const devices = useDevices();
  const [list, setList] = useState();
  const [error, setError] = useState();
  const [editing, setEditing] = useState(null);
  const load = async () => {
    try {
      setList(await api.automation.bindings.list());
    } catch (e) {
      setError(e);
    }
  };
  useEffect(() => {
    load();
  }, []);

  const automations = [...(motion || []), ...(schedules || [])].map((a) => a.name);
  const kinds = Object.fromEntries((outputs || []).map((o) => [o.name, o.kind]));

  if (editing)
    return html`<${BindingEditor} ...${editing} devices=${devices} outputs=${outputs || []} automations=${automations}
      done=${(saved) => {
        setEditing(null);
        if (saved) load();
      }} />`;

  const remove = async (slot) => {
    if (!confirm('Delete this binding?')) return;
    if ((await run(() => api.automation.bindings.remove(slot), 'Binding deleted')) !== undefined) load();
  };
  // By switch, then button and event.
  const sorted = (list || []).slice().sort(
    (a, b) =>
      deviceName(devices, a.trigger.device).localeCompare(deviceName(devices, b.trigger.device)) ||
      a.trigger.button - b.trigger.button ||
      EVENTS.findIndex(([k]) => k === a.trigger.event) - EVENTS.findIndex(([k]) => k === b.trigger.event),
  );

  return html`<${Section} title="Buttons" aside=${html`<button class="primary" onClick=${() => setEditing({})}>+ New binding</button>`}>
    ${!list
      ? html`<${Loading} error=${error} />`
      : list.length === 0
        ? html`<p class="muted">No button does anything yet. A binding ties a button event (pressed once, twice, held …) to up to four actions.</p>`
        : html`<div class="table-wrap"><table>
            <thead><tr><th>Switch</th><th>Button</th><th>When</th><th>Does</th><th></th></tr></thead>
            <tbody>
              ${sorted.map(
                ({ slot, trigger, actions }) => html`<tr key=${slot}>
                  <td>${deviceName(devices, trigger.device)}</td>
                  <td>${trigger.button}</td>
                  <td>${eventLabel(trigger.event)}</td>
                  <td class="wrap">${actions.map((a) => actionText(a, devices, kinds)).join(' · ')}</td>
                  <td class="actions">
                    <button onClick=${() => setEditing({ slot, original: { trigger, actions } })}>Edit</button>
                    <button class="danger" onClick=${() => remove(slot)}>Delete</button>
                  </td>
                </tr>`,
              )}
            </tbody>
          </table></div>`}
  <//>`;
}

