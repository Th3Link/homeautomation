// The gateway's own automation (ADR 0025): an overview to test and watch
// it, and a tab per building block.
import { useState } from 'preact/hooks';
import * as api from '../../api.js';
import { Link } from '../../router.js';
import { html, run, notify, useLoad, Section, Loading } from '../../ui.js';
import { MotionLights, Schedules } from './automations.js';
import { deviceName, fromYaml, toYaml, useDevices } from './common.js';
import { Outputs } from './outputs.js';
import { eventLabel, loadSwitch, saveSwitch, switchFromText, switchToText, Switches } from './switches.js';

const TABS = [
  ['', 'Overview'],
  ['switches', 'Switches'],
  ['outputs', 'Outputs'],
  ['motion', 'Motion lights'],
  ['schedules', 'Schedules'],
  ['backup', 'Backup'],
];

// Everything the tabs show, in one go (one request after the other).
async function loadAll() {
  const status = await api.automation.status();
  const outputs = await api.automation.outputs.list();
  const motion = await api.automation.motion.list();
  const schedules = await api.automation.schedules.list();
  return { status, outputs, motion, schedules };
}

export function Automation({ tab = '' }) {
  // The overview follows the states; the editors don't need to.
  const { data, error, reload } = useLoad(loadAll, tab === '' ? 2500 : 0);
  const d = data || {};
  return html`
    <nav class="tabs">
      ${TABS.map(([t, label]) => html`<${Link} href=${'/automation' + (t ? '/' + t : '')} class=${t === tab ? 'active' : ''}>${label}<//>`)}
    </nav>
    ${!data && html`<${Loading} error=${error} />`}
    ${data && tab === '' && html`<${Overview} ...${d} reload=${reload} />`}
    ${data && tab === 'switches' && html`<${Switches} outputs=${d.outputs} motion=${d.motion} schedules=${d.schedules} />`}
    ${data && tab === 'outputs' && html`<${Outputs} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'motion' && html`<${MotionLights} motion=${d.motion} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'schedules' && html`<${Schedules} schedules=${d.schedules} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'backup' && html`<${Backup} ...${d} reload=${reload} />`}
    ${error && data && html`<p class="error">${error.message}</p>`}`;
}

// ---- overview ------------------------------------------------------------

// Sends `op` at once and shows the expected state until the gateway's
// next answer: the controls never wait for the round trip.
function useCommand(o, reload) {
  const [pending, setPending] = useState(null);
  const send = async (op, expected) => {
    setPending(expected);
    await run(() => api.automation.outputs.command(o.name, op));
    setTimeout(async () => {
      await reload();
      setPending(null);
    }, 600);
  };
  return [pending === null ? o.level : pending, send];
}

function LevelControl({ o, reload }) {
  const [level, send] = useCommand(o, reload);
  const top = o.kind === 'light' ? o.top : 255;
  const step = o.kind === 'light' ? 1 : 25;
  const set = (n) => send({ set: n }, Math.max(0, Math.min(top, n)));
  return html`<div class="stepper">
      <button aria-label="Darker" disabled=${level <= 0} onClick=${() => send('dec', Math.max(0, level - step))}>−</button>
      ${o.kind === 'light'
        ? html`<select aria-label="Level" value=${level} onChange=${(e) => set(+e.target.value)}>
            ${Array.from({ length: top + 1 }, (_, i) => html`<option value=${i}>${i === 0 ? 'off' : `level ${i}`}</option>`)}
          </select>`
        : html`<input type="number" aria-label="Value 0–255" min="0" max="255" value=${level} onChange=${(e) => set(+e.target.value)} />`}
      <button aria-label="Brighter" disabled=${level >= top} onClick=${() => send('inc', Math.min(top, level + step))}>+</button>
    </div>
    <div class="buttons fixed"><button onClick=${() => send('on', level || 1)}>On</button><button onClick=${() => send('off', 0)}>Off</button></div>`;
}

function OutputTile({ o, reload }) {
  const [, send] = useCommand(o, reload);
  let state;
  let controls;
  switch (o.kind) {
    case 'light':
    case 'dimmer':
      state = o.level > 0 ? (o.kind === 'light' ? `level ${o.level} of ${o.top}` : `${Math.round((o.level / 255) * 100)} %`) : 'off';
      controls = html`<${LevelControl} o=${o} reload=${reload} />`;
      break;
    case 'relay':
      state = o.level > 0 ? 'on' : 'off';
      controls = html`<div class="buttons fixed"><button onClick=${() => send('on', 1)}>On</button><button onClick=${() => send('off', 0)}>Off</button></div>`;
      break;
    case 'shutter':
      state = { up: 'moving up', down: 'moving down', stopped: 'stopped' }[o.movement];
      controls = html`<div class="buttons fixed">
        <button aria-label="Up" onClick=${() => send('up', o.level)}>▲</button>
        <button aria-label="Stop" onClick=${() => send('stop', o.level)}>■</button>
        <button aria-label="Down" onClick=${() => send('down', o.level)}>▼</button>
      </div>`;
      break;
    default:
      state = 'group';
      controls = html`<div class="buttons fixed"><button onClick=${() => send('on', 0)}>All on</button><button onClick=${() => send('off', 0)}>All off</button></div>`;
  }
  const lit = o.level > 0 || (o.kind === 'shutter' && o.movement !== 'stopped');
  return html`<div class=${'tile' + (lit ? ' lit' : '')}>
    <div class="tile-head"><strong>${o.name}</strong></div>
    <div class="tile-state">${state}</div>
    ${controls}
  </div>`;
}

const STATES = { idle: 'waiting', auto: 'on by motion', manual: 'on by hand', lockout: 'locked', running: 'in a time window' };

function Overview({ status, outputs, motion, schedules, reload }) {
  const devices = useDevices();
  const setEnabled = async (enabled) => {
    await run(() => api.automation.setEnabled(enabled), enabled ? 'Automation on' : 'Automation off');
    reload();
  };
  const toggle = (calls, a) => async () => {
    await run(() => calls.setEnabled(a.name, !a.enabled), `${a.name} ${a.enabled ? 'disabled' : 'enabled'}`);
    setTimeout(reload, 400);
  };
  const last = status.last_button;
  const nothing = !outputs.length && !motion.length && !schedules.length;
  const groups = outputs.filter((o) => o.kind === 'group');
  const others = outputs.filter((o) => o.kind !== 'group');
  return html`
    <${Section} title="Automation" aside=${html`<label class="switch big" title="The master switch">
      <input type="checkbox" checked=${status.enabled} onChange=${(e) => setEnabled(e.target.checked)} /><span></span>
      ${status.enabled ? 'On' : 'Off'}
    </label>`}>
      <p class="muted">
        Switches, motion lights and schedules run on the gateway itself: they keep working without the network, MQTT or Home Assistant.
        This page is for testing and watching them.
        ${!status.enabled && html` <strong>Switched off:</strong> nothing switches by itself; outputs can still be switched here.`}
      </p>
      <dl class="facts">
        <div><dt>Switch configurations</dt><dd><${Link} href="/automation/switches">${status.switches}<//></dd></div>
        <div><dt>Outputs</dt><dd><${Link} href="/automation/outputs">${status.outputs}<//></dd></div>
        <div><dt>Motion lights</dt><dd><${Link} href="/automation/motion">${status.motion}<//></dd></div>
        <div><dt>Schedules</dt><dd><${Link} href="/automation/schedules">${status.schedules}<//></dd></div>
        <div><dt>Last button pressed</dt>
        <dd>${last ? `${deviceName(devices, last.trigger.device)}, button ${last.trigger.button}, ${eventLabel(last.trigger.event).toLowerCase()} (${last.age_s} s ago)` : '—'}</dd></div>
      </dl>
      ${nothing &&
      html`<ol class="steps">
        <li>Create the <${Link} href="/automation/outputs">outputs<//>: the lights, relays and shutters the automation switches.</li>
        <li>Tell the <${Link} href="/automation/switches">switches<//> what their buttons do.</li>
        <li>Add <${Link} href="/automation/motion">motion lights<//> and <${Link} href="/automation/schedules">schedules<//>.</li>
        <li>Or restore a <${Link} href="/automation/backup">backup<//>.</li>
      </ol>`}
    <//>
    ${groups.length > 0 &&
    html`<${Section} title="Groups">
      <div class="tiles">${groups.map((o) => html`<${OutputTile} key=${o.name} o=${o} reload=${reload} />`)}</div>
    <//>`}
    ${others.length > 0 &&
    html`<${Section} title="Outputs">
      <div class="tiles">${others.map((o) => html`<${OutputTile} key=${o.name} o=${o} reload=${reload} />`)}</div>
    <//>`}
    ${motion.length + schedules.length > 0 &&
    html`<${Section} title="Motion lights and schedules">
      <div class="tiles">
        ${[...motion.map((a) => [a, api.automation.motion]), ...schedules.map((a) => [a, api.automation.schedules])].map(
          ([a, calls]) => html`<div class=${'tile' + (a.enabled && a.state !== 'idle' ? ' lit' : '')} key=${a.name}>
            <div class="tile-head"><strong>${a.name}</strong></div>
            <div class="tile-state">${a.enabled ? STATES[a.state] : 'disabled'}${a.enabled && a.remaining_s !== undefined ? ` · ${a.remaining_s} s` : ''}</div>
            <label class="switch"><input type="checkbox" checked=${a.enabled} onChange=${toggle(calls, a)} /><span></span>${a.enabled ? 'Enabled' : 'Disabled'}</label>
          </div>`,
        )}
      </div>
    <//>`}`;
}

// ---- backup --------------------------------------------------------------

const FORMAT = 'gateway-automation';
const VERSION = 2;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The whole configuration as one object (the backup file).
async function collect(status, outputs, motion, schedules) {
  const file = { format: FORMAT, version: VERSION, enabled: status.enabled, outputs: [], switches: [], motion: [], schedules: [], devices: [] };
  for (const o of outputs) file.outputs.push({ name: o.name, output: await api.automation.outputs.get(o.name) });
  for (const c of await api.automation.switches.list()) file.switches.push({ name: c.name, ...switchToText(await loadSwitch(c.name, c.buttons)) });
  for (const m of motion) file.motion.push({ name: m.name, enabled: m.enabled, config: await api.automation.motion.get(m.name) });
  for (const s of schedules) file.schedules.push({ name: s.name, enabled: s.enabled, config: await api.automation.schedules.get(s.name) });
  for (const d of await api.automation.devices.list()) file.devices.push({ device: d.device, ...(await api.automation.devices.get(d.device)) });
  return file;
}

function download(text, name, type) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type }));
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

function Backup({ status, outputs, motion, schedules, reload }) {
  const [log, setLog] = useState([]);
  const [busy, setBusy] = useState(false);
  const [replace, setReplace] = useState(false);
  const [text, setText] = useState(null);
  const say = (line) => setLog((l) => [...l, line]);

  const exportAs = async (yaml) => {
    setBusy(true);
    setLog([]);
    try {
      const file = await collect(status, outputs, motion, schedules);
      const day = new Date().toISOString().slice(0, 10);
      if (yaml) download(toYaml(file), `automation-${day}.yaml`, 'application/yaml');
      else download(JSON.stringify(file, null, 2), `automation-${day}.json`, 'application/json');
      say(`Exported ${file.outputs.length} outputs, ${file.switches.length} switch configurations, ${file.motion.length} motion lights, ${file.schedules.length} schedules.`);
    } catch (e) {
      say(`Failed: ${e.message}`);
    }
    setBusy(false);
  };

  const editAll = async () => {
    setBusy(true);
    try {
      setText(toYaml(await collect(status, outputs, motion, schedules)));
    } catch (e) {
      notify(e.message, 'error');
    }
    setBusy(false);
  };

  // Every step, with what went wrong, into the log; carries on with the next.
  const step = async (label, call) => {
    try {
      await call();
      say(`✓ ${label}`);
    } catch (e) {
      say(`✗ ${label}: ${e.field ? e.field + ': ' : ''}${e.message}`);
    }
  };

  const apply = async (file) => {
    if (!file || file.format !== FORMAT) return notify('Not an automation backup', 'error');
    if (file.version !== VERSION) return notify(`Version ${file.version} isn't supported (this page writes ${VERSION})`, 'error');
    if (replace && !confirm('Delete every output, switch configuration, motion light and schedule on the gateway first?')) return;
    setBusy(true);
    setLog([]);
    if (replace) {
      for (const c of await api.automation.switches.list()) await step(`delete switch ${c.name}`, () => api.automation.switches.remove(c.name));
      for (const m of motion) await step(`delete ${m.name}`, () => api.automation.motion.remove(m.name));
      for (const s of schedules) await step(`delete ${s.name}`, () => api.automation.schedules.remove(s.name));
      const groupsFirst = [...outputs].sort((a, b) => (b.kind === 'group') - (a.kind === 'group'));
      for (const o of groupsFirst) await step(`delete ${o.name}`, () => api.automation.outputs.remove(o.name));
    }
    // Groups after their members; switches last (they name the others).
    const isGroup = (o) => Object.keys(o.output)[0] === 'group';
    const outs = file.outputs || [];
    for (const o of [...outs.filter((o) => !isGroup(o)), ...outs.filter(isGroup)]) await step(`output ${o.name}`, () => api.automation.outputs.put(o.name, o.output));
    for (const m of file.motion || []) await step(`motion light ${m.name}`, () => api.automation.motion.put(m.name, m.config));
    for (const s of file.schedules || []) await step(`schedule ${s.name}`, () => api.automation.schedules.put(s.name, s.config));
    for (const c of file.switches || []) await step(`switch ${c.name}`, () => saveSwitch(c.name, switchFromText(c)));
    for (const d of file.devices || [])
      await step(`device notes ${deviceName([], d.device)}`, () => api.automation.devices.put(d.device, { features: d.features || [], lamps: d.lamps || [] }));
    // New ones start enabled; the gateway loads them a moment after saving.
    const disabled = [...(file.motion || []).map((m) => [m, api.automation.motion]), ...(file.schedules || []).map((s) => [s, api.automation.schedules])].filter(([a]) => a.enabled === false);
    if (disabled.length) await sleep(1000);
    for (const [a, calls] of disabled) await step(`disable ${a.name}`, () => calls.setEnabled(a.name, false));
    if (typeof file.enabled === 'boolean' && file.enabled !== status.enabled)
      await step(`master switch ${file.enabled ? 'on' : 'off'}`, () => api.automation.setEnabled(file.enabled));
    say('Done.');
    setBusy(false);
    reload();
  };

  const importFile = async (e) => {
    const f = e.target.files[0];
    e.target.value = '';
    if (!f) return;
    try {
      // YAML reads JSON too.
      await apply(fromYaml(await f.text()));
    } catch (err) {
      notify(`${f.name}: ${err.message}`, 'error');
    }
  };

  const applyText = async () => {
    let file;
    try {
      file = fromYaml(text);
    } catch (e) {
      return notify(e.message, 'error');
    }
    await apply(file);
    setText(null);
  };

  return html`<${Section} title="Backup">
    <p class="muted">The whole configuration as one file (YAML or JSON): to keep, to edit, to write by hand, or to move to another gateway. Devices are stored by type and id.</p>
    <div class="buttons">
      <button class="primary" disabled=${busy} onClick=${() => exportAs(true)}>Export YAML</button>
      <button disabled=${busy} onClick=${() => exportAs(false)}>Export JSON</button>
      <label class=${'button' + (busy ? ' disabled' : '')}>Import…<input type="file" accept=".json,.yaml,.yml,application/json,application/yaml" hidden disabled=${busy} onChange=${importFile} /></label>
      <button disabled=${busy || text !== null} onClick=${editAll}>Edit everything as YAML</button>
      <label class="check"><input type="checkbox" checked=${replace} onChange=${(e) => setReplace(e.target.checked)} /> Delete everything first</label>
    </div>
    ${text !== null &&
    html`<div class="text-all">
      <textarea class="code" rows="28" spellcheck="false" value=${text} onInput=${(e) => setText(e.target.value)} />
      <div class="buttons">
        <button class="primary" disabled=${busy} onClick=${applyText}>Apply</button>
        <button onClick=${() => setText(null)}>Cancel</button>
        <small class="muted">Applying stores every item as written; with “Delete everything first”, what isn't in the text is removed.</small>
      </div>
    </div>`}
    ${log.length > 0 && html`<pre class="log">${log.join('\n')}</pre>`}
  <//>`;
}
