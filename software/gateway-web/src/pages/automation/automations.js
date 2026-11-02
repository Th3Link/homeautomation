// Motion lights and schedules: the automations that switch by themselves.
import { useState } from 'preact/hooks';
import * as api from '../../api.js';
import { html, run, notify, Section, Loading, Field } from '../../ui.js';
import {
  ConditionEdit,
  conditionText,
  DeviceSelect,
  deviceName,
  Editor,
  errorText,
  ListEdit,
  TimeEdit,
  timeText,
  useDevices,
  useFeatures,
  withFeature,
} from './common.js';

const NAME = /^[A-Za-z0-9_-]{1,20}$/;
const DAYS = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];

// Outputs a motion light or schedule can switch (not shutters or groups).
const switchable = (outputs) => (outputs || []).filter((o) => ['light', 'dimmer', 'relay'].includes(o.kind));

function OutputSelect({ value, onChange, outputs }) {
  return html`<select value=${value} onChange=${(e) => onChange(e.target.value)}>
    ${!outputs.some((o) => o.name === value) && html`<option value=${value}>${value || 'Choose…'}</option>`}
    ${outputs.map((o) => html`<option value=${o.name}>${o.name}</option>`)}
  </select>`;
}

// The editor both kinds share: a name, then the kind's own fields.
function NamedEditor({ title, intro, oldName, initial, api: calls, done, children }) {
  const [name, setName] = useState(oldName || '');
  const [item, setItem] = useState(initial);
  const [error, setError] = useState('');
  const save = async (text) => {
    const it = text || item;
    if (!NAME.test(name)) return setError('Name: 1–20 letters, digits, - and _');
    if (JSON.stringify(it).includes('"sensor":{"type":0,')) return setError('Choose the brightness sensor');
    // Detector rows that weren't filled in.
    const clean = it.detectors ? { ...it, detectors: it.detectors.filter(Boolean) } : it;
    try {
      await calls.put(name, clean);
      if (oldName && oldName !== name) await calls.remove(oldName);
      notify(`${name} saved`);
      done(true);
    } catch (e) {
      setError(errorText(e));
    }
  };
  return html`<${Editor} title=${oldName ? `${title} ${oldName}` : `New ${title.toLowerCase()}`} onSave=${save} onCancel=${() => done(false)} error=${error}
    value=${item} onText=${setItem} intro=${intro}>
    <div class="row">
      <${Field} label="Name" hint="Letters, digits, - and _; also the MQTT topic and the switch in Home Assistant">
        <input value=${name} maxlength="20" onInput=${(e) => setName(e.target.value.trim())} />
      <//>
    </div>
    ${children(item, setItem)}
  <//>`;
}

// ---- motion lights -------------------------------------------------------

const newMotion = (outputs) => ({
  detectors: [],
  output: outputs[0] ? outputs[0].name : '',
  timeout_s: 180,
  lockout_s: 60,
  levels: [],
  conditions: [],
});

const MOTION_INTRO =
  'Motion switches the output on; it goes off again after a while without motion. ' +
  'Switched on by hand (a button, Home Assistant), it stays on until switched off. ' +
  'After it went off, motion is ignored for a moment, so it does not come straight back on for someone leaving.';

// The level from a time of day on: fixed, or the last one with a minimum.
function PeriodEdit({ value, onChange }) {
  const fixed = value.level !== undefined && value.level !== null;
  return html`<span class="period">
    <small>From</small><${TimeEdit} value=${value.from} onChange=${(from) => onChange({ ...value, from })} />
    <select aria-label="Level" value=${fixed ? 'fixed' : 'last'} onChange=${(e) =>
      onChange(e.target.value === 'fixed' ? { from: value.from, level: 1 } : { from: value.from, min: 1 })}>
      <option value="fixed">always level</option>
      <option value="last">the last level, at least</option>
    </select>
    <input type="number" class="short" min="1" max="255" aria-label="Level" value=${fixed ? value.level : value.min || 0}
      onInput=${(e) => onChange(fixed ? { from: value.from, level: +e.target.value } : { from: value.from, min: +e.target.value })} />
  </span>`;
}

function MotionFields({ m, set, outputs, devices, features }) {
  const patch = (p) => set({ ...m, ...p });
  return html`
    <div class="row">
      <${Field} label="Switches"><${OutputSelect} value=${m.output} outputs=${outputs} onChange=${(output) => patch({ output })} /><//>
      <${Field} label="Off after no motion for (min)">
        <input type="number" min="0.1" max="1440" step="0.5" value=${m.timeout_s / 60} onInput=${(e) => patch({ timeout_s: Math.max(1, Math.round(+e.target.value * 60)) })} />
      <//>
      <${Field} label="Then ignore motion for (s)" hint="So it doesn't come straight back on for someone leaving">
        <input type="number" min="0" max="3600" value=${m.lockout_s} onInput=${(e) => patch({ lockout_s: +e.target.value })} />
      <//>
    </div>
    <h3>Motion detectors</h3>
    <p class="muted">Switches with a PIR (marked on their device page).</p>
    <${ListEdit} items=${m.detectors} max=${4} addLabel="motion detector" onChange=${(detectors) => patch({ detectors })}
      make=${() => null}
      render=${(d, setD) => html`<${DeviceSelect} value=${d} devices=${withFeature(devices, features, 'pir')} onChange=${setD} />`} />
    <h3>Level by the time of day</h3>
    <p class="muted">Without any: the level it had last. E.g. from 22:00 always level 1; from sunrise the last level, but at least 4.</p>
    <${ListEdit} items=${m.levels || []} max=${4} addLabel="time of day" onChange=${(levels) => patch({ levels })}
      make=${() => ({ from: { clock: { hour: 22, minute: 0 } }, level: 1 })}
      render=${(p, setP) => html`<${PeriodEdit} value=${p} onChange=${setP} />`} />
    <h3>Only switches on if</h3>
    <p class="muted">All of these hold. One that can't be told (no reading, no clock) counts as holding. Switching it on by hand stops the timer until it is switched off.</p>
    <${ListEdit} items=${m.conditions || []} max=${3} addLabel="condition" onChange=${(conditions) => patch({ conditions })}
      make=${() => ({ sun_below: { degrees: -6 } })}
      render=${(c, setC) => html`<${ConditionEdit} value=${c} devices=${devices} onChange=${setC} />`} />`;
}

// ---- schedules -----------------------------------------------------------

const SCHEDULE_INTRO =
  'Switches an output on and off in time windows: by the clock, relative to the sun, and off early when it gets bright. ' +
  'Only the start and the end switch, so switching it off by hand in between sticks. Sun times use the location on the Gateway page.';

const newWindow = () => ({ start: { sun: { event: 'civil_dusk', offset_min: -30 } }, end: [{ at: { clock: { hour: 23, minute: 0 } } }], days: 0x7f });

function EndEdit({ value, onChange, devices }) {
  const bright = !!value.bright;
  return html`<span class="end">
    <select value=${bright ? 'bright' : 'at'} onChange=${(e) =>
      onChange(e.target.value === 'bright' ? { bright: { sensor: { type: 0, id: 1 }, lux: 200, for_min: 5 } } : { at: { clock: { hour: 8, minute: 0 } } })}>
      <option value="at">At</option>
      <option value="bright">When bright</option>
    </select>
    ${bright
      ? html`<input type="number" class="short" min="0" value=${value.bright.lux} onInput=${(e) => onChange({ bright: { ...value.bright, lux: +e.target.value } })} /><small>lx for</small>
          <input type="number" class="short" min="0" max="600" value=${value.bright.for_min} onInput=${(e) => onChange({ bright: { ...value.bright, for_min: +e.target.value } })} /><small>min at</small>
          <${DeviceSelect} value=${value.bright.sensor.type ? value.bright.sensor : null} devices=${devices} onChange=${(sensor) => onChange({ bright: { ...value.bright, sensor } })} />`
      : html`<${TimeEdit} value=${value.at} onChange=${(at) => onChange({ at })} />`}
  </span>`;
}

function WindowEdit({ w, set, devices }) {
  const patch = (p) => set({ ...w, ...p });
  return html`<div class="window">
    <div class="row">
      <${Field} label="On at"><${TimeEdit} value=${w.start} onChange=${(start) => patch({ start })} /><//>
      <div class="field"><span>Days</span>
        <div class="days">
          ${DAYS.map(
            (d, i) => html`<label class=${'lamp' + (w.days & (1 << i) ? ' on' : '')} key=${d}>
              <input type="checkbox" checked=${!!(w.days & (1 << i))} onChange=${() => patch({ days: w.days ^ (1 << i) })} />${d}
            </label>`,
          )}
        </div>
      </div>
    </div>
    <div class="row tight">
      <label class="check"><input type="checkbox" checked=${!!w.only_if} onChange=${(e) => {
        const { only_if, ...rest } = w;
        set(e.target.checked ? { ...rest, only_if: { sun_below: { degrees: -6 } } } : rest);
      }} /> Only if, at the start:</label>
      ${w.only_if && html`<${ConditionEdit} value=${w.only_if} devices=${devices} onChange=${(only_if) => patch({ only_if })} />`}
    </div>
    <span class="label">Off at the first of</span>
    <${ListEdit} items=${w.end} max=${3} addLabel="end" onChange=${(end) => patch({ end })}
      make=${() => ({ at: { clock: { hour: 8, minute: 0 } } })}
      render=${(e, setE) => html`<${EndEdit} value=${e} devices=${devices} onChange=${setE} />`} />
  </div>`;
}

function ScheduleFields({ s, set, outputs, devices }) {
  return html`
    <div class="row">
      <${Field} label="Switches" hint="On at its last level"><${OutputSelect} value=${s.output} outputs=${outputs} onChange=${(output) => set({ ...s, output })} /><//>
    </div>
    <h3>Time windows</h3>
    <p class="muted">Only the start and the end switch: switching it off by hand in between sticks. Nothing switches while the clock isn't set.</p>
    <${ListEdit} items=${s.windows} max=${4} addLabel="time window" onChange=${(windows) => set({ ...s, windows })}
      make=${newWindow}
      render=${(w, setW) => html`<${WindowEdit} w=${w} set=${setW} devices=${devices} />`} />`;
}

function windowText(w, devices) {
  const days = w.days === 0x7f ? '' : ` (${DAYS.filter((_, i) => w.days & (1 << i)).join(' ')})`;
  const cond = w.only_if ? ` if ${conditionText(w.only_if, devices)}` : '';
  const ends = w.end.map((e) => (e.at ? timeText(e.at) : `≥ ${e.bright.lux} lx`)).join(' / ');
  return `${timeText(w.start)}${cond} → ${ends}${days}`;
}

// ---- the lists -----------------------------------------------------------

const MODE = { idle: ['Waiting', 'muted'], auto: ['On by motion', 'ok'], manual: ['On by hand', 'warn'], lockout: ['Locked', 'muted'], running: ['Running', 'ok'] };

function List({ kind, title, items, outputs, reload, empty }) {
  const devices = useDevices();
  const features = useFeatures();
  const [editing, setEditing] = useState(null);
  const calls = kind === 'motion' ? api.automation.motion : api.automation.schedules;
  const [configs, setConfigs] = useState({});

  const edit = async (name) => {
    const initial = name ? await run(() => calls.get(name)) : kind === 'motion' ? newMotion(outputs) : { output: outputs[0] ? outputs[0].name : '', windows: [newWindow()] };
    if (!initial) return;
    setEditing({ oldName: name, initial });
  };
  const toggle = async (a) => {
    await run(() => calls.setEnabled(a.name, !a.enabled), `${a.name} ${a.enabled ? 'disabled' : 'enabled'}`);
    setTimeout(reload, 300);
  };
  const remove = async (name) => {
    if (!confirm(`Delete ${name}?`)) return;
    if ((await run(() => calls.remove(name), `${name} deleted`)) !== undefined) setTimeout(reload, 300);
  };
  const show = async (name) => {
    if (configs[name]) return setConfigs({ ...configs, [name]: undefined });
    const c = await run(() => calls.get(name));
    if (c) setConfigs({ ...configs, [name]: c });
  };

  if (editing)
    return html`<${NamedEditor} title=${title} ...${editing} api=${calls} intro=${kind === 'motion' ? MOTION_INTRO : SCHEDULE_INTRO}
      done=${(saved) => {
        setEditing(null);
        if (saved) setTimeout(reload, 300);
      }}>
      ${(item, set) =>
        kind === 'motion'
          ? html`<${MotionFields} m=${item} set=${set} outputs=${outputs} devices=${devices} features=${features} />`
          : html`<${ScheduleFields} s=${item} set=${set} outputs=${outputs} devices=${devices} />`}
    <//>`;

  return html`<${Section} title=${title + 's'} aside=${html`<button class="primary" onClick=${() => edit()}>+ New ${title.toLowerCase()}</button>`}>
    ${!items
      ? html`<${Loading} />`
      : items.length === 0
        ? html`<p class="muted">${empty}</p>`
        : html`<div class="table-wrap"><table>
            <thead><tr><th>Name</th><th>State</th><th>Enabled</th><th></th></tr></thead>
            <tbody>
              ${items.map((a) => {
                const [label, cls] = MODE[a.state] || [a.state, 'muted'];
                const c = configs[a.name];
                return html`<tr key=${a.name}>
                    <td><a href="#" onClick=${(e) => (e.preventDefault(), show(a.name))}><strong>${a.name}</strong></a></td>
                    <td><span class=${'badge ' + (a.enabled ? cls : 'muted')}>${label}</span>${a.remaining_s !== undefined ? html` <small class="muted">${Math.ceil(a.remaining_s / 60)} min</small>` : ''}</td>
                    <td><label class="switch"><input type="checkbox" checked=${a.enabled} onChange=${() => toggle(a)} /><span></span></label></td>
                    <td class="actions">
                      <button onClick=${() => edit(a.name)}>Edit</button>
                      <button class="danger" onClick=${() => remove(a.name)}>Delete</button>
                    </td>
                  </tr>
                  ${c &&
                  html`<tr class="details"><td colspan="4">
                    ${kind === 'motion'
                      ? `${c.output}${(c.levels || []).length ? ' (' + c.levels.map((p) => `from ${timeText(p.from)} ${p.level ? 'level ' + p.level : 'at least ' + p.min}`).join(', ') + ')' : ''}; off ${c.timeout_s / 60} min after the last motion at ${c.detectors.map((d) => deviceName(devices, d)).join(', ')}; locked ${c.lockout_s} s${(c.conditions || []).length ? '; only if ' + c.conditions.map((x) => conditionText(x, devices)).join(' and ') : ''}`
                      : html`${c.output}: ${c.windows.map((w) => html`<div>${windowText(w, devices)}</div>`)}`}
                  </td></tr>`}`;
              })}
            </tbody>
          </table></div>`}
  <//>`;
}

export const MotionLights = ({ motion, outputs, reload }) => html`<${List} kind="motion" title="Motion light" items=${motion} outputs=${switchable(outputs)} reload=${reload}
  empty="None yet. A motion light switches an output on by motion and off after a while without it; it can be bound to a button (three times down: off) and switched in Home Assistant." />`;

export const Schedules = ({ schedules, outputs, reload }) => html`<${List} kind="schedule" title="Schedule" items=${schedules} outputs=${switchable(outputs)} reload=${reload}
  empty="None yet. A schedule switches an output in time windows: by the clock, relative to the sun, and off when it gets bright." />`;
