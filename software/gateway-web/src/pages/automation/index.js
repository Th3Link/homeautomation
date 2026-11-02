// The gateway's own automation (ADR 0025): an overview to watch and
// control it, and a tab per building block.
import { useState } from 'preact/hooks';
import * as api from '../../api.js';
import { Link } from '../../router.js';
import { html, run, notify, useLoad, Section, Loading } from '../../ui.js';
import { MotionLights, Schedules } from './automations.js';
import { Bindings, EVENTS } from './bindings.js';
import { deviceName, useDevices } from './common.js';
import { Outputs } from './outputs.js';

const TABS = [
  ['', 'Overview'],
  ['outputs', 'Outputs'],
  ['buttons', 'Buttons'],
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
    ${data && tab === 'outputs' && html`<${Outputs} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'buttons' && html`<${Bindings} outputs=${d.outputs} motion=${d.motion} schedules=${d.schedules} />`}
    ${data && tab === 'motion' && html`<${MotionLights} motion=${d.motion} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'schedules' && html`<${Schedules} schedules=${d.schedules} outputs=${d.outputs} reload=${reload} />`}
    ${data && tab === 'backup' && html`<${Backup} ...${d} reload=${reload} />`}
    ${error && data && html`<p class="error">${error.message}</p>`}`;
}

// ---- overview ------------------------------------------------------------

const command = (name, op, reload) => async () => {
  await run(() => api.automation.outputs.command(name, op));
  setTimeout(reload, 400);
};

function OutputTile({ o, reload }) {
  const on = o.level > 0;
  const c = (op) => command(o.name, op, reload);
  let state;
  let controls;
  switch (o.kind) {
    case 'light':
      state = on ? `level ${o.level} of ${o.top}` : 'off';
      controls = html`
        <input type="range" min="0" max=${o.top} value=${o.level} onChange=${(e) => command(o.name, { set: +e.target.value }, reload)()} />
        <div class="buttons"><button onClick=${c('dec')}>−</button><button onClick=${c('inc')}>+</button><button onClick=${c(on ? 'off' : 'on')}>${on ? 'Off' : 'On'}</button></div>`;
      break;
    case 'dimmer':
      state = on ? `${Math.round((o.level / 255) * 100)} %` : 'off';
      controls = html`
        <input type="range" min="0" max="255" value=${o.level} onChange=${(e) => command(o.name, { set: +e.target.value }, reload)()} />
        <div class="buttons"><button onClick=${c(on ? 'off' : 'on')}>${on ? 'Off' : 'On'}</button></div>`;
      break;
    case 'relay':
      state = on ? 'on' : 'off';
      controls = html`<div class="buttons"><button onClick=${c(on ? 'off' : 'on')}>${on ? 'Off' : 'On'}</button></div>`;
      break;
    case 'shutter':
      state = { up: 'moving up', down: 'moving down', stopped: 'stopped' }[o.movement];
      controls = html`<div class="buttons"><button title="Up" onClick=${c('up')}>▲</button><button title="Stop" onClick=${c('stop')}>■</button><button title="Down" onClick=${c('down')}>▼</button></div>`;
      break;
    default:
      state = 'group';
      controls = html`<div class="buttons"><button onClick=${c('on')}>On</button><button onClick=${c('off')}>Off</button></div>`;
  }
  const lit = on || (o.kind === 'shutter' && o.movement !== 'stopped');
  return html`<div class=${'tile' + (lit ? ' lit' : '')}>
    <div class="tile-head"><strong>${o.name}</strong><small>${state}</small></div>
    ${controls}
  </div>`;
}

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
  return html`
    <${Section} title="Automation" aside=${html`<label class="switch big" title="The master switch">
      <input type="checkbox" checked=${status.enabled} onChange=${(e) => setEnabled(e.target.checked)} /><span></span>
      ${status.enabled ? 'On' : 'Off'}
    </label>`}>
      <p class="muted">
        Buttons, motion lights and schedules run on the gateway itself: they keep working without the network, MQTT or Home Assistant.
        ${!status.enabled && html` <strong>Switched off:</strong> nothing switches by itself, outputs can still be switched here.`}
      </p>
      <dl class="facts">
        <div><dt>Outputs</dt><dd><${Link} href="/automation/outputs">${status.outputs}<//></dd></div>
        <div><dt>Button bindings</dt><dd><${Link} href="/automation/buttons">${status.bindings}<//></dd></div>
        <div><dt>Motion lights</dt><dd><${Link} href="/automation/motion">${status.motion}<//></dd></div>
        <div><dt>Schedules</dt><dd><${Link} href="/automation/schedules">${status.schedules}<//></dd></div>
        <div><dt>Last button</dt>
        <dd>${last
          ? `${deviceName(devices, last.trigger.device)}, button ${last.trigger.button}, ${(EVENTS.find(([k]) => k === last.trigger.event) || [, last.trigger.event])[1].toLowerCase()} (${last.age_s} s ago)`
          : '—'}</dd></div>
      </dl>
      ${nothing && html`<p>Start with the <${Link} href="/automation/outputs">outputs<//>: what the automation switches. Or restore a <${Link} href="/automation/backup">backup<//>.</p>`}
    <//>
    ${outputs.length > 0 &&
    html`<${Section} title="Outputs">
      <div class="tiles">${outputs.map((o) => html`<${OutputTile} key=${o.name} o=${o} reload=${reload} />`)}</div>
    <//>`}
    ${motion.length + schedules.length > 0 &&
    html`<${Section} title="Motion lights and schedules">
      <div class="tiles">
        ${[...motion.map((a) => [a, api.automation.motion]), ...schedules.map((a) => [a, api.automation.schedules])].map(
          ([a, calls]) => html`<div class=${'tile' + (a.enabled && a.state !== 'idle' ? ' lit' : '')} key=${a.name}>
            <div class="tile-head"><strong>${a.name}</strong>
              <small>${a.enabled ? { idle: 'waiting', auto: 'on by motion', manual: 'on by hand', lockout: 'locked', running: 'running' }[a.state] : 'disabled'}
              ${a.enabled && a.remaining_s !== undefined ? ` · ${a.remaining_s} s` : ''}</small>
            </div>
            <label class="switch"><input type="checkbox" checked=${a.enabled} onChange=${toggle(calls, a)} /><span></span>${a.enabled ? 'Enabled' : 'Disabled'}</label>
          </div>`,
        )}
      </div>
    <//>`}`;
}

// ---- backup --------------------------------------------------------------

const FORMAT = 'gateway-automation';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function Backup({ status, outputs, motion, schedules, reload }) {
  const [log, setLog] = useState([]);
  const [busy, setBusy] = useState(false);
  const [replace, setReplace] = useState(false);
  const say = (line) => setLog((l) => [...l, line]);

  const exportAll = async () => {
    setBusy(true);
    setLog([]);
    try {
      const file = { format: FORMAT, version: 1, enabled: status.enabled, outputs: [], bindings: [], motion: [], schedules: [] };
      for (const o of outputs) file.outputs.push({ name: o.name, output: await api.automation.outputs.get(o.name) });
      file.bindings = (await api.automation.bindings.list()).map(({ trigger, actions }) => ({ trigger, actions }));
      for (const m of motion) file.motion.push({ name: m.name, enabled: m.enabled, config: await api.automation.motion.get(m.name) });
      for (const s of schedules) file.schedules.push({ name: s.name, enabled: s.enabled, config: await api.automation.schedules.get(s.name) });
      const blob = new Blob([JSON.stringify(file, null, 2)], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = `automation-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      say(`Exported ${file.outputs.length} outputs, ${file.bindings.length} bindings, ${file.motion.length} motion lights, ${file.schedules.length} schedules.`);
    } catch (e) {
      say(`Failed: ${e.message}`);
    }
    setBusy(false);
  };

  // Every step, with what went wrong, into the log; carries on with the next.
  const step = async (label, call) => {
    try {
      await call();
      say(`✓ ${label}`);
      return true;
    } catch (e) {
      say(`✗ ${label}: ${e.field ? e.field + ': ' : ''}${e.message}`);
      return false;
    }
  };

  const importFile = async (e) => {
    const f = e.target.files[0];
    e.target.value = '';
    if (!f) return;
    let file;
    try {
      file = JSON.parse(await f.text());
      if (file.format !== FORMAT) throw new Error('not an automation backup');
    } catch (err) {
      return notify(`${f.name}: ${err.message}`, 'error');
    }
    if (replace && !confirm('Delete every output, binding, motion light and schedule on the gateway first?')) return;
    setBusy(true);
    setLog([]);
    if (replace) {
      for (const b of await api.automation.bindings.list()) await step(`delete binding ${b.slot}`, () => api.automation.bindings.remove(b.slot));
      for (const m of motion) await step(`delete ${m.name}`, () => api.automation.motion.remove(m.name));
      for (const s of schedules) await step(`delete ${s.name}`, () => api.automation.schedules.remove(s.name));
      const groupsFirst = [...outputs].sort((a, b) => (b.kind === 'group') - (a.kind === 'group'));
      for (const o of groupsFirst) await step(`delete ${o.name}`, () => api.automation.outputs.remove(o.name));
    }
    // Groups after their members, bindings last (they name the others).
    const isGroup = (o) => Object.keys(o.output)[0] === 'group';
    for (const o of [...(file.outputs || []).filter((o) => !isGroup(o)), ...(file.outputs || []).filter(isGroup)])
      await step(`output ${o.name}`, () => api.automation.outputs.put(o.name, o.output));
    for (const m of file.motion || []) await step(`motion light ${m.name}`, () => api.automation.motion.put(m.name, m.config));
    for (const s of file.schedules || []) await step(`schedule ${s.name}`, () => api.automation.schedules.put(s.name, s.config));
    for (const [i, b] of (file.bindings || []).entries()) await step(`binding ${i + 1}`, () => api.automation.bindings.add(b));
    // New ones start enabled; the gateway loads them a moment after saving.
    const disabled = [...(file.motion || []).map((m) => [m, api.automation.motion]), ...(file.schedules || []).map((s) => [s, api.automation.schedules])].filter(([a]) => a.enabled === false);
    if (disabled.length) await sleep(1000);
    for (const [a, calls] of disabled) await step(`disable ${a.name}`, () => calls.setEnabled(a.name, false));
    say('Done.');
    setBusy(false);
    reload();
  };

  return html`<${Section} title="Backup">
    <p class="muted">The whole configuration as one JSON file: to keep, to edit, or to move to another gateway. Devices are stored by type and id.</p>
    <div class="buttons">
      <button class="primary" disabled=${busy} onClick=${exportAll}>Export</button>
      <label class=${'button' + (busy ? ' disabled' : '')}>Import…<input type="file" accept=".json,application/json" hidden disabled=${busy} onChange=${importFile} /></label>
      <label class="check"><input type="checkbox" checked=${replace} onChange=${(e) => setReplace(e.target.checked)} /> Delete everything first</label>
    </div>
    ${log.length > 0 && html`<pre class="log">${log.join('\n')}</pre>`}
  <//>`;
}
