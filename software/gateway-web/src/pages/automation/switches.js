// Switch configurations: what the buttons of a wall switch do, for one
// switch or several that do the same. One item per configuration, stored
// on the gateway as the switch list plus one part per button.
import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from '../../api.js';
import { html, run, notify, Section, Loading, Field } from '../../ui.js';
import {
  DeviceSelect,
  deviceName,
  Editor,
  errorText,
  hasFeature,
  ListEdit,
  OpSelect,
  opText,
  refKey,
  useDevices,
  useFeatures,
  withFeature,
} from './common.js';

export const EVENTS = [
  ['single', 'Once', 'Pressed once'],
  ['double', 'Twice', 'Pressed twice'],
  ['triple', '3 times', 'Pressed three times'],
  ['hold', 'Held', 'Held: repeats every 0.5 s'],
  ['released', 'Let go', 'Let go after holding'],
];
export const eventLabel = (e) => (EVENTS.find(([k]) => k === e) || [e, e, e])[2];
const BUTTONS = 8;
const MAX_ACTIONS = 4;
const MAX_SWITCHES = 8;
const NAME = /^[A-Za-z0-9_-]{1,20}$/;
// Devices of type 4 are the wall switches.
const SWITCH_TYPES = [4];

// A configuration as the page holds it: `buttons[b][event]` = actions.
const emptyButtons = () => Array.from({ length: BUTTONS }, () => ({}));
const fromApi = (events) => Object.fromEntries((events || []).map((e) => [e.event, e.actions]));
const toApi = (byEvent) => ({
  events: EVENTS.map(([k]) => k)
    .filter((k) => (byEvent[k] || []).length)
    .map((event) => ({ event, actions: byEvent[event] })),
});

// The text form (YAML and the backup file): buttons by number, events by
// name, only those that do something.
export function switchToText(config) {
  const buttons = {};
  config.buttons.forEach((b, i) => {
    const used = Object.fromEntries(Object.entries(b).filter(([, a]) => a && a.length));
    if (Object.keys(used).length) buttons[i] = used;
  });
  return { switches: config.switches, buttons };
}
export function switchFromText(text) {
  const buttons = emptyButtons();
  for (const [b, events] of Object.entries(text.buttons || {})) {
    if (+b >= 0 && +b < BUTTONS) buttons[+b] = { ...events };
  }
  return { switches: text.switches || [], buttons };
}

// Loads a configuration with its buttons (`used`: the bits from the list).
export async function loadSwitch(name, used) {
  const config = await api.automation.switches.get(name);
  const buttons = emptyButtons();
  for (let b = 0; b < BUTTONS; b++) {
    if (used & (1 << b)) buttons[b] = fromApi((await api.automation.switches.button(name, b)).events);
  }
  return { switches: config.switches, buttons };
}

// Stores a configuration; only the buttons that changed against `before`
// (all of them for a new name).
export async function saveSwitch(name, config, before) {
  await api.automation.switches.put(name, { switches: config.switches });
  for (let b = 0; b < BUTTONS; b++) {
    const now = JSON.stringify(toApi(config.buttons[b]));
    const was = before ? JSON.stringify(toApi(before.buttons[b])) : '{"events":[]}';
    if (now !== was) await api.automation.switches.putButton(name, b, toApi(config.buttons[b]));
  }
}

// ---- actions --------------------------------------------------------------

function actionText(a, devices, kinds) {
  if (a.output) return `${a.output.name}: ${opText(a.output.op, kinds[a.output.name]).toLowerCase()}`;
  if (a.automation) return `${a.automation.name}: ${a.automation.switch}`;
  if (a.nightlight) return `night light ${deviceName(devices, a.nightlight.device)}: ${a.nightlight.value}`;
  return '?';
}

function ActionEdit({ value, onChange, devices, features, outputs, automations }) {
  const type = Object.keys(value)[0];
  const v = value[type];
  const kinds = Object.fromEntries(outputs.map((o) => [o.name, o.kind]));
  const switchTo = (t) => {
    if (t === 'output') onChange({ output: { name: outputs[0] ? outputs[0].name : '', op: 'toggle' } });
    if (t === 'automation') onChange({ automation: { name: automations[0] || '', switch: 'toggle' } });
    if (t === 'nightlight') onChange({ nightlight: { device: { type: 4, id: 1 }, value: 0 } });
  };
  return html`<span class="action">
    <select value=${type} onChange=${(e) => switchTo(e.target.value)} aria-label="Kind of action">
      <option value="output">Output</option>
      <option value="automation">Motion light / schedule</option>
      <option value="nightlight">Night light</option>
    </select>
    ${type === 'output' &&
    html`<select value=${v.name} aria-label="Output" onChange=${(e) => onChange({ output: { name: e.target.value, op: 'toggle' } })}>
        ${!outputs.some((o) => o.name === v.name) && html`<option value=${v.name}>${v.name || 'Choose…'}</option>`}
        ${outputs.map((o) => html`<option value=${o.name}>${o.name}</option>`)}
      </select>
      <${OpSelect} value=${v.op} kind=${kinds[v.name]} onChange=${(op) => onChange({ output: { ...v, op } })} />`}
    ${type === 'automation' &&
    html`<select value=${v.name} aria-label="Motion light or schedule" onChange=${(e) => onChange({ automation: { ...v, name: e.target.value } })}>
        ${!automations.includes(v.name) && html`<option value=${v.name}>${v.name || 'Choose…'}</option>`}
        ${automations.map((n) => html`<option value=${n}>${n}</option>`)}
      </select>
      <select value=${v.switch} aria-label="Switch it" onChange=${(e) => onChange({ automation: { ...v, switch: e.target.value } })}>
        <option value="toggle">Toggle</option>
        <option value="enable">Enable</option>
        <option value="disable">Disable</option>
      </select>`}
    ${type === 'nightlight' &&
    html`<${DeviceSelect} value=${v.device} devices=${withFeature(devices, features, 'nightlight')} types=${SWITCH_TYPES}
        onChange=${(device) => onChange({ nightlight: { ...v, device } })} />
      <input type="number" class="short" min="0" max="255" aria-label="Brightness 0–255" value=${v.value}
        onInput=${(e) => onChange({ nightlight: { ...v, value: +e.target.value } })} />`}
  </span>`;
}

// ---- learning -------------------------------------------------------------

// Waits for the next button press on the bus and hands its trigger
// (`{device, button, event}`) to `onLearn`.
export function Learn({ onLearn, label = 'Learn: press a button' }) {
  const [waiting, setWaiting] = useState(false);
  const stop = useRef(false);
  useEffect(() => () => (stop.current = true), []);
  const start = async () => {
    setWaiting(true);
    stop.current = false;
    const t0 = Date.now();
    while (!stop.current && Date.now() < t0 + 30000) {
      try {
        const s = await api.automation.status();
        // Pressed after we started waiting (the age is in whole seconds).
        if (s.last_button && s.last_button.age_s + 1 <= (Date.now() - t0) / 1000) {
          onLearn(s.last_button.trigger);
          break;
        }
      } catch (e) {}
      await new Promise((r) => setTimeout(r, 700));
    }
    setWaiting(false);
  };
  return waiting
    ? html`<button class="learning" onClick=${() => (stop.current = true)}>Press the button on the wall now… (cancel)</button>`
    : html`<button onClick=${start}>${label}</button>`;
}

// ---- templates ------------------------------------------------------------

// A rocker (two buttons, up first) for one output, as the old rules did.
function rocker(kind, name) {
  const act = (op) => [{ output: { name, op } }];
  if (kind === 'shutter')
    return [
      { single: act('up'), hold: act('up_hold'), released: act('stop') },
      { single: act('down'), hold: act('down_hold'), released: act('stop') },
    ];
  if (kind === 'relay' || kind === 'group') return [{ single: act('on') }, { single: act('off') }];
  return [
    { single: act('on'), double: act('max'), hold: act('inc') },
    { single: act('off'), hold: act('dec') },
  ];
}

function RockerTemplate({ outputs, onApply, buttons }) {
  const [pair, setPair] = useState(0);
  const [name, setName] = useState(outputs[0] ? outputs[0].name : '');
  const kind = (outputs.find((o) => o.name === name) || {}).kind;
  return html`<div class="template">
    <span class="label">Quick fill:</span>
    <select value=${pair} aria-label="Buttons" onChange=${(e) => setPair(+e.target.value)}>
      ${Array.from({ length: buttons / 2 }, (_, i) => html`<option value=${i}>buttons ${2 * i} (up) and ${2 * i + 1} (down)</option>`)}
    </select>
    <span class="label">as a rocker for</span>
    <select value=${name} aria-label="Output" onChange=${(e) => setName(e.target.value)}>
      ${outputs.map((o) => html`<option value=${o.name}>${o.name}</option>`)}
    </select>
    <button disabled=${!name} onClick=${() => onApply(2 * pair, rocker(kind, name))}>Fill</button>
    <small class="muted">${kind === 'shutter' ? 'Up/down, held: moves, let go: stops' : kind === 'light' || kind === 'dimmer' ? 'Up: on, twice: brightest, held: brighter; down: off, held: darker' : 'Up: on, down: off'}</small>
  </div>`;
}

// ---- the editor -----------------------------------------------------------

function SwitchEditor({ name: oldName, original, devices, features, outputs, automations, done, learned }) {
  const [name, setName] = useState(oldName || '');
  const [config, setConfig] = useState(original || { switches: learned ? [learned.device] : [], buttons: emptyButtons() });
  const [cell, setCell] = useState(learned ? [learned.button, learned.event] : [0, 'single']);
  const [error, setError] = useState('');
  const kinds = Object.fromEntries(outputs.map((o) => [o.name, o.kind]));
  const extension =
    config.switches.some((s) => hasFeature(features, s, 'extension')) || config.buttons.slice(4).some((b) => Object.keys(b).length);
  const [showAll, setShowAll] = useState(false);
  const count = extension || showAll || cell[0] >= 4 ? BUTTONS : 4;
  const [b, ev] = cell;
  const actions = config.buttons[b][ev] || [];
  const setActions = (next) =>
    setConfig({ ...config, buttons: config.buttons.map((x, i) => (i === b ? { ...x, [ev]: next } : x)) });

  const learn = (t) => {
    const known = config.switches.some((s) => refKey(s) === refKey(t.device));
    if (!known && config.switches.length < MAX_SWITCHES) setConfig((c) => ({ ...c, switches: [...c.switches, t.device] }));
    setCell([t.button, t.event]);
    notify(`Button ${t.button}, ${eventLabel(t.event).toLowerCase()}`);
  };

  const save = async (text) => {
    const c = text ? switchFromText(text) : config;
    if (!NAME.test(name)) return setError('Name: 1–20 letters, digits, - and _');
    if (!c.switches.length) return setError('Add at least one switch, or learn it');
    try {
      const renamed = oldName && oldName !== name;
      await saveSwitch(name, c, renamed ? undefined : original);
      if (renamed) await api.automation.switches.remove(oldName);
      notify(`${name} saved`);
      done(true);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return html`<${Editor} title=${oldName ? `Switch ${oldName}` : 'New switch configuration'} onSave=${save} onCancel=${() => done(false)} error=${error}
    value=${switchToText(config)} onText=${(t) => setConfig(switchFromText(t))}
    intro="What each button does. Several switches can share one configuration (both bedroom switches): they all do the same.">
    <div class="row">
      <${Field} label="Name" hint="Letters, digits, - and _">
        <input value=${name} maxlength="20" onInput=${(e) => setName(e.target.value.trim())} />
      <//>
      <div class="field grow"><span>Switches</span>
        <${ListEdit} items=${config.switches} max=${MAX_SWITCHES} addLabel="switch" onChange=${(switches) => setConfig({ ...config, switches })}
          make=${() => null}
          render=${(d, setD) => html`<${DeviceSelect} value=${d} devices=${devices} types=${SWITCH_TYPES} onChange=${setD} />`} />
      </div>
    </div>
    <div class="row tight">
      <${Learn} onLearn=${learn} label="Learn: press a button on the wall" />
      <small class="muted">Adds the switch and picks the button and kind of press.</small>
    </div>

    <h3>Buttons</h3>
    <div class="table-wrap"><table class="grid">
      <thead><tr><th>Button</th>${EVENTS.map(([k, short, long]) => html`<th title=${long}>${short}</th>`)}</tr></thead>
      <tbody>
        ${Array.from({ length: count }, (_, i) => html`<tr key=${i}>
          <th scope="row">${i}${i >= 4 ? html` <small class="muted">ext.</small>` : ''}</th>
          ${EVENTS.map(([k]) => {
            const a = config.buttons[i][k] || [];
            const active = b === i && ev === k;
            return html`<td key=${k}><button class=${'cell' + (active ? ' active' : '') + (a.length ? ' used' : '')}
              aria-pressed=${active} onClick=${() => setCell([i, k])}>
              ${a.length ? a.map((x) => actionText(x, devices, kinds)).join(', ') : '—'}
            </button></td>`;
          })}
        </tr>`)}
      </tbody>
    </table></div>
    ${!extension && count === 4 && html`<button class="link" onClick=${() => setShowAll(true)}>Show buttons 4–7 (extension board)</button>`}

    <div class="cell-edit">
      <h3>Button ${b}, ${eventLabel(ev).toLowerCase()}</h3>
      <${ListEdit} items=${actions} max=${MAX_ACTIONS} addLabel="action" onChange=${setActions}
        make=${() => ({ output: { name: outputs[0] ? outputs[0].name : '', op: 'toggle' } })}
        render=${(a, set) => html`<${ActionEdit} value=${a} onChange=${set} devices=${devices} features=${features} outputs=${outputs} automations=${automations} />`} />
      ${actions.length === 0 && html`<p class="muted">Does nothing. Add an action, or fill a rocker below.</p>`}
    </div>
    ${outputs.length > 0 &&
    html`<${RockerTemplate} outputs=${outputs} buttons=${count}
      onApply=${(first, rows) => {
        setConfig({ ...config, buttons: config.buttons.map((x, i) => (i === first || i === first + 1 ? { ...x, ...rows[i - first] } : x)) });
        setCell([first, 'single']);
      }} />`}
  <//>`;
}

// ---- the list -------------------------------------------------------------

export function Switches({ outputs, motion, schedules }) {
  const devices = useDevices();
  const features = useFeatures();
  const [list, setList] = useState();
  const [error, setError] = useState();
  const [editing, setEditing] = useState(null);
  const load = async () => {
    try {
      setList(await api.automation.switches.list());
    } catch (e) {
      setError(e);
    }
  };
  useEffect(() => {
    load();
  }, []);
  const automations = [...(motion || []), ...(schedules || [])].map((a) => a.name);

  const edit = async (item) => {
    const original = await run(() => loadSwitch(item.name, item.buttons));
    if (original) setEditing({ name: item.name, original });
  };
  // A press on the wall: the configuration of that switch, or a new one.
  const find = (t) => {
    const item = (list || []).find((c) => c.switches.some((s) => refKey(s) === refKey(t.device)));
    if (item) edit(item);
    else setEditing({ learned: t });
  };
  const remove = async (name) => {
    if (!confirm(`Delete the switch configuration ${name}? Its buttons stop doing anything.`)) return;
    if ((await run(() => api.automation.switches.remove(name), `${name} deleted`)) !== undefined) load();
  };

  if (editing)
    return html`<${SwitchEditor} ...${editing} devices=${devices} features=${features} outputs=${outputs || []} automations=${automations}
      done=${(saved) => {
        setEditing(null);
        if (saved) load();
      }} />`;

  return html`<${Section} title="Switches" aside=${html`<div class="buttons">
      <${Learn} onLearn=${find} label="Which switch is this?" />
      <button class="primary" onClick=${() => setEditing({})}>+ New</button>
    </div>`}>
    <p class="muted">A configuration says what each button of a switch does when pressed once, twice, three times, held or let go. Switches that do the same share one.</p>
    ${!list
      ? html`<${Loading} error=${error} />`
      : list.length === 0
        ? html`<p>No switch does anything yet. Press <em>Which switch is this?</em> and a button on the wall to start with that switch.</p>`
        : html`<div class="table-wrap"><table>
            <thead><tr><th>Name</th><th>Switches</th><th>Buttons</th><th></th></tr></thead>
            <tbody>
              ${list.map(
                (c) => html`<tr key=${c.name}>
                  <td><strong>${c.name}</strong></td>
                  <td class="wrap">${c.switches.map((s) => deviceName(devices, s)).join(', ')}</td>
                  <td><span class="chips">${Array.from({ length: BUTTONS }, (_, i) => i)
                    .filter((i) => c.buttons & (1 << i))
                    .map((i) => html`<span class="chip">${i}</span>`)}</span></td>
                  <td class="actions">
                    <button onClick=${() => edit(c)}>Edit</button>
                    <button class="danger" onClick=${() => remove(c.name)}>Delete</button>
                  </td>
                </tr>`,
              )}
            </tbody>
          </table></div>`}
  <//>`;
}
