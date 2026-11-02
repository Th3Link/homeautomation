// What the automation switches: lights in levels, dimmers, relays, roller
// shutters and groups of them, each by name.
import { useEffect, useState } from 'preact/hooks';
import * as api from '../../api.js';
import { html, run, notify, Section, Loading, Field } from '../../ui.js';
import { DeviceSelect, deviceName, devicePath, Editor, errorText, ListEdit, OUTPUT_KINDS, useDevices } from './common.js';

// LegacyLamps, the lamp controllers.
const LAMP_TYPES = [3];
const MAX_GROUPS = 8;
const MAX_LEVELS = 15;

const kindOf = (output) => Object.keys(output)[0];
const kindLabel = (kind) => (OUTPUT_KINDS.find(([k]) => k === kind) || [kind, kind])[1];

const blank = {
  light: () => ({ light: { groups: [{ device: { type: 3, id: 1 }, mask: 1 }], levels: [[40], [80], [127]], default_level: 2 } }),
  dimmer: () => ({ dimmer: { lamps: { device: { type: 3, id: 1 }, mask: 1 }, default_value: 127 } }),
  relay: () => ({ relay: { device: { type: 5, id: 1 }, channel: 0, bank: 0 } }),
  shutter: () => ({ shutter: { device: { type: 5, id: 1 }, channel: 0, bank: 0, travel_ms: 20000, up: 1 } }),
  group: () => ({ group: { members: [] } }),
};

// What an output does, in one line.
function summary(output, devices) {
  const kind = kindOf(output);
  const o = output[kind];
  switch (kind) {
    case 'light':
      return `${o.levels.length} levels, ${o.groups.length} lamp group${o.groups.length > 1 ? 's' : ''} on ${[...new Set(o.groups.map((g) => deviceName(devices, g.device)))].join(', ')}`;
    case 'dimmer':
      return `${deviceName(devices, o.lamps.device)}, mask 0x${o.lamps.mask.toString(16)}`;
    case 'relay':
      return `${deviceName(devices, o.device)}, channel ${o.channel}`;
    case 'shutter':
      return `${deviceName(devices, o.device)}, shutter ${o.channel}, ${o.travel_ms / 1000} s`;
    case 'group':
      return o.members.join(', ');
  }
  return '';
}

// ---- lamp masks ----------------------------------------------------------

// The names of a lamp controller's lamps (by bit), from its device notes.
function useLampNames(device) {
  const [names, setNames] = useState([]);
  const key = device ? `${device.type}:${device.id}` : '';
  const load = () => device && api.automation.devices.get(device).then((n) => setNames(n.lamps || []), () => setNames([]));
  useEffect(() => {
    load();
  }, [key]);
  return [names, load];
}

// The 24 bits of a lamp mask as toggles (bit 23 first, as on the device
// page), with the lamps' names. `lit` marks the lamp being explored.
function MaskEdit({ value, onChange, lit, onPick, names = [] }) {
  return html`<div class="lamps named">
    ${Array.from({ length: 24 }, (_, i) => 23 - i).map((bit) => {
      const on = (value >>> bit) & 1;
      const pick = () => (onPick ? onPick(bit) : onChange((value ^ (1 << bit)) >>> 0));
      return html`<label key=${bit} title=${names[bit] || `Lamp ${bit}`} class=${'lamp' + (on ? ' on' : '') + (bit === lit ? ' lit' : '')}>
        <input type="checkbox" checked=${!!on} onChange=${pick} /><b>${bit}</b>${names[bit] && html`<small>${names[bit]}</small>`}
      </label>`;
    })}
  </div>`;
}

const EXPLORE_VALUE = 127;

// Finds out which lamp a bit is: lights that one lamp on the controller
// (and switches the one before off), so it can be seen in the room.
function Explore({ group, onChange, devices, names, reloadNames }) {
  const [lit, setLit] = useState(null);
  const [label, setLabel] = useState('');
  const path = devicePath(devices, group.device);
  const bank = group.bank ?? 0;
  const send = (value, bitmask) => run(() => api.devices.lamps(path, bank, value, bitmask));
  const light = async (bit) => {
    if (lit !== null && lit !== bit) await send(0, 1 << lit);
    await send(EXPLORE_VALUE, 1 << bit);
    setLit(bit);
    setLabel(names[bit] || '');
  };
  // Names the lit lamp, in the controller's device notes.
  const saveName = async () => {
    const notes = await run(() => api.automation.devices.get(group.device));
    if (!notes) return;
    const lamps = Array.from({ length: 24 }, (_, i) => (notes.lamps || [])[i] || '');
    lamps[lit] = label.trim().slice(0, 16);
    while (lamps.length && !lamps[lamps.length - 1]) lamps.pop();
    if ((await run(() => api.automation.devices.put(group.device, { ...notes, lamps }), `Lamp ${lit} named`)) !== undefined) reloadNames();
  };
  const stop = async () => {
    if (lit !== null) await send(0, 1 << lit);
    setLit(null);
  };
  const toggle = () => onChange({ ...group, mask: (group.mask ^ (1 << lit)) >>> 0 });
  const member = lit !== null && (group.mask >>> lit) & 1;
  return html`<div class="explore">
    <p class="muted">Click a lamp: it lights up in the room (the one before goes off). The controller takes about one change per second. Give it a name while it's lit: the name is kept with the controller and shown everywhere.</p>
    <${MaskEdit} value=${group.mask} lit=${lit} onPick=${light} names=${names} />
    ${lit !== null &&
    html`<div class="row tight">
      <input aria-label=${`Name of lamp ${lit}`} placeholder=${`Name of lamp ${lit}, e.g. “living white 2”`} maxlength="16" value=${label} onInput=${(e) => setLabel(e.target.value)} />
      <button onClick=${saveName}>Save name</button>
    </div>`}
    <div class="buttons">
      <button class="primary" disabled=${lit === null} onClick=${toggle}>
        ${lit === null ? 'Click a lamp first' : member ? `Remove lamp ${lit} from the group` : `Add lamp ${lit} to the group`}
      </button>
      <button disabled=${!group.mask} onClick=${() => send(EXPLORE_VALUE, group.mask)}>Light the whole group</button>
      <button onClick=${stop}>Done: switch off</button>
    </div>
  </div>`;
}

function LampGroupEdit({ value, onChange, devices }) {
  const [exploring, setExploring] = useState(false);
  const [names, reloadNames] = useLampNames(value.device);
  return html`<div class="row">
    <${Field} label="Lamp controller">
      <${DeviceSelect} value=${value.device} devices=${devices} types=${LAMP_TYPES} onChange=${(device) => onChange({ ...value, device })} />
    <//>
    <${Field} label="Bank" hint="Empty: the controller's only one">
      <input type="number" min="0" max="255" value=${value.bank ?? ''} onInput=${(e) => {
        const { bank, ...rest } = value;
        onChange(e.target.value === '' ? rest : { ...rest, bank: +e.target.value });
      }} />
    <//>
    <div class="field wide">
      <span>Lamps (mask 0x${value.mask.toString(16).padStart(6, '0')})
        <button class="link" onClick=${() => setExploring(!exploring)}>${exploring ? 'Close' : 'Find a lamp…'}</button></span>
      ${exploring
        ? html`<${Explore} group=${value} devices=${devices} onChange=${onChange} names=${names} reloadNames=${reloadNames} />`
        : html`<${MaskEdit} value=${value.mask} names=${names} onChange=${(mask) => onChange({ ...value, mask })} />`}
    </div>
  </div>`;
}

// ---- lights --------------------------------------------------------------

// Fills the levels with a ramp from `from` to `to` (exponent 3 like the
// openHAB rule: fine steps at the dark end).
function ramp(count, groups, from, to, exponent) {
  return Array.from({ length: count }, (_, i) => {
    const v = Math.round(from + (to - from) * Math.pow((i + 1) / count, exponent));
    return Array(groups).fill(Math.max(1, Math.min(255, v)));
  });
}

function LightEdit({ value, onChange, devices }) {
  const [rampTo, setRampTo] = useState(127);
  const groups = value.groups;
  const setGroups = (next) => {
    // Every level keeps one value per group.
    const levels = value.levels.map((l) => next.map((_, i) => (i < l.length ? l[i] : l[l.length - 1] ?? 0)));
    onChange({ ...value, groups: next, levels });
  };
  const setValue = (level, group, v) =>
    onChange({ ...value, levels: value.levels.map((l, i) => (i === level ? l.map((x, j) => (j === group ? v : x)) : l)) });
  // Shows a level on the real lamps, before saving.
  const tryLevel = async (values) => {
    for (const [j, g] of groups.entries()) {
      await run(() => api.devices.lamps(devicePath(devices, g.device), g.bank ?? 0, values ? values[j] : 0, g.mask));
    }
  };
  const setCount = (n) => {
    const levels = value.levels.slice(0, n);
    while (levels.length < n) levels.push([...(levels[levels.length - 1] || groups.map(() => 0))]);
    onChange({ ...value, levels, default_level: Math.min(value.default_level, n) });
  };
  return html`
    <h3>Lamp groups</h3>
    <p class="muted">Lamps that always have the same value in every level form a group. A light may span several controllers.</p>
    <${ListEdit} items=${groups} max=${MAX_GROUPS} addLabel="lamp group" onChange=${setGroups}
      make=${() => ({ device: groups.length ? groups[groups.length - 1].device : { type: 3, id: 1 }, mask: 1 })}
      render=${(g, set) => html`<${LampGroupEdit} value=${g} devices=${devices} onChange=${set} />`} />

    <h3>Levels</h3>
    <p class="muted">The value (0–255) of each lamp group per level; level 0 is off. The controllers fade by themselves and take one change per second.</p>
    <div class="row">
      <${Field} label="Levels">
        <input type="number" min="1" max=${MAX_LEVELS} value=${value.levels.length} onInput=${(e) => e.target.value && setCount(Math.max(1, Math.min(MAX_LEVELS, +e.target.value)))} />
      <//>
      <${Field} label="Default level" hint="What “on” switches to the first time">
        <select value=${value.default_level} onChange=${(e) => onChange({ ...value, default_level: +e.target.value })}>
          ${value.levels.map((_, i) => html`<option value=${i + 1}>${i + 1}</option>`)}
        </select>
      <//>
      <${Field} label="Ramp up to" hint="Fills every level, all groups alike">
        <input type="number" min="1" max="255" value=${rampTo} onInput=${(e) => setRampTo(+e.target.value)} />
      <//>
      <button onClick=${() => onChange({ ...value, levels: ramp(value.levels.length, groups.length, 2, rampTo, 3) })}>Fill ramp</button>
    </div>
    <div class="table-wrap"><table class="levels">
      <thead><tr><th>Level</th>${groups.map((_, i) => html`<th>Group ${i + 1}</th>`)}<th><button class="link" onClick=${() => tryLevel(null)}>All off</button></th></tr></thead>
      <tbody>
        ${value.levels.map(
          (l, i) => html`<tr key=${i}>
            <td>${i + 1}${i + 1 === value.default_level && html` <span class="badge muted">default</span>`}</td>
            ${l.map((v, j) => html`<td><input type="number" class="short" min="0" max="255" aria-label=${`Level ${i + 1}, group ${j + 1}`} value=${v} onInput=${(e) => setValue(i, j, +e.target.value)} /></td>`)}
            <td><button onClick=${() => tryLevel(l)} title="Show this level on the lamps">Try</button></td>
          </tr>`,
        )}
      </tbody>
    </table></div>`;
}

// ---- the other kinds -----------------------------------------------------

const num = (label, value, set, attrs = {}, hint) => html`<${Field} label=${label} hint=${hint}>
  <input type="number" value=${value} ...${attrs} onInput=${(e) => set(+e.target.value)} />
<//>`;

function KindEdit({ output, onChange, devices, names, kinds = {} }) {
  const kind = kindOf(output);
  const kindOf_ = (n) => kinds[n];
  const o = output[kind];
  const set = (patch) => onChange({ [kind]: { ...o, ...patch } });
  switch (kind) {
    case 'light':
      return html`<${LightEdit} value=${o} devices=${devices} onChange=${(v) => onChange({ light: v })} />`;
    case 'dimmer':
      return html`<${LampGroupEdit} value=${o.lamps} devices=${devices} onChange=${(lamps) => set({ lamps })} />
        <div class="row">${num('Default value', o.default_value, (default_value) => set({ default_value }), { min: 1, max: 255 }, 'What “on” switches to the first time')}</div>`;
    case 'relay':
      return html`<div class="row">
        <${Field} label="Relay node"><${DeviceSelect} value=${o.device} devices=${devices} onChange=${(device) => set({ device })} /><//>
        ${num('Channel', o.channel, (channel) => set({ channel }), { min: 0, max: 255 })}
        ${num('Bank', o.bank, (bank) => set({ bank }), { min: 0, max: 255 })}
      </div>`;
    case 'shutter':
      return html`<div class="row">
        <${Field} label="Shutter node"><${DeviceSelect} value=${o.device} devices=${devices} onChange=${(device) => set({ device })} /><//>
        ${num('Shutter', o.channel, (channel) => set({ channel }), { min: 0, max: 255 })}
        ${num('Bank', o.bank, (bank) => set({ bank }), { min: 0, max: 255 })}
      </div>
      <div class="row">
        ${num('Travel time (s)', o.travel_ms / 1000, (s) => set({ travel_ms: Math.round(s * 1000) }), { min: 1, max: 600, step: 0.5 }, 'A full move; sent with up and down')}
        <${Field} label="Up is" hint="Depends on the wiring: swap it if up and down are the wrong way round">
          <select value=${o.up} onChange=${(e) => set({ up: +e.target.value })}>
            <option value="1">Direction 1</option>
            <option value="2">Direction 2</option>
          </select>
        <//>
      </div>`;
    case 'group':
      return html`<p class="muted">Switched together, e.g. “all lights” for one button that switches everything off. Up to every other output; groups can't contain groups.</p>
        <div class="buttons">
          <button onClick=${() => set({ members: names })}>All</button>
          <button onClick=${() => set({ members: names.filter((n) => kindOf_(n) === 'light' || kindOf_(n) === 'dimmer') })}>All lights</button>
          <button onClick=${() => set({ members: [] })}>None</button>
        </div>
        <div class="checks">
          ${names.map(
            (n) => html`<label class="check" key=${n}>
              <input type="checkbox" checked=${o.members.includes(n)} onChange=${(e) =>
                set({ members: e.target.checked ? [...o.members, n] : o.members.filter((m) => m !== n) })} /> ${n}
            </label>`,
          )}
        </div>`;
  }
  return null;
}

// ---- the page ------------------------------------------------------------

const INTRO = {
  light: 'A room light in levels: lamp groups on one or more lamp controllers, and the value of each group per level. “On” goes back to the level it had last.',
  dimmer: 'One lamp group switched with a value 0–255, without a level table.',
  relay: 'A relay channel of a relay node: on and off.',
  shutter: 'A roller shutter on a relay node: up, down and stop. Pressing up while it moves up stops it.',
  group: 'Outputs switched together: on, off, toggle, and up/down for the shutters in it.',
};

function OutputEditor({ original, name: oldName, outputs, devices, done }) {
  const [name, setName] = useState(oldName || '');
  const [output, setOutput] = useState(original || blank.light());
  const [error, setError] = useState('');
  const kind = kindOf(output);
  const memberNames = outputs.filter((o) => o.kind !== 'group' && o.name !== oldName).map((o) => o.name);

  const save = async (text) => {
    if (!/^[A-Za-z0-9_-]{1,20}$/.test(name)) return setError('Name: 1–20 letters, digits, - and _');
    try {
      await api.automation.outputs.put(name, text || output);
      if (oldName && oldName !== name) await api.automation.outputs.remove(oldName);
      notify(`${name} saved`);
      done(true);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return html`<${Editor} title=${oldName ? `Output ${oldName}` : 'New output'} onSave=${save} onCancel=${() => done(false)} error=${error}
    value=${output} onText=${setOutput} intro=${INTRO[kind]}>
    <div class="row">
      <${Field} label="Name" hint=${oldName && name !== oldName ? 'Renaming: bindings and automations that use the old name stop working' : 'Letters, digits, - and _'}>
        <input value=${name} maxlength="20" onInput=${(e) => setName(e.target.value.trim())} />
      <//>
      <${Field} label="Kind">
        <select value=${kind} disabled=${!!oldName} onChange=${(e) => setOutput(blank[e.target.value]())}>
          ${OUTPUT_KINDS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
        </select>
      <//>
    </div>
    <${KindEdit} output=${output} onChange=${setOutput} devices=${devices} names=${memberNames}
      kinds=${Object.fromEntries(outputs.map((o) => [o.name, o.kind]))} />
  <//>`;
}

export function Outputs({ outputs, reload }) {
  const devices = useDevices();
  const [editing, setEditing] = useState(null);
  const [details, setDetails] = useState({});

  const [version, setVersion] = useState(0);
  const names = (outputs || []).map((o) => o.name).join(',');

  // The configurations, for the summaries (loaded one by one: the gateway
  // serves few connections at a time).
  useEffect(() => {
    let stopped = false;
    (async () => {
      const all = {};
      for (const name of names ? names.split(',') : []) {
        try {
          all[name] = await api.automation.outputs.get(name);
        } catch (e) {}
        if (stopped) return;
      }
      setDetails(all);
    })();
    return () => (stopped = true);
  }, [names, version]);

  const edit = async (name) => {
    const original = name ? await run(() => api.automation.outputs.get(name)) : undefined;
    if (name && !original) return;
    setEditing({ name, original });
  };
  const remove = async (name) => {
    if (!confirm(`Delete the output ${name}? Bindings and automations that use it stop working.`)) return;
    if ((await run(() => api.automation.outputs.remove(name), `${name} deleted`)) !== undefined) reload();
  };

  if (editing)
    return html`<${OutputEditor} ...${editing} outputs=${outputs || []} devices=${devices}
      done=${(saved) => {
        setEditing(null);
        if (saved) {
          setVersion((v) => v + 1);
          setTimeout(reload, 300);
        }
      }} />`;

  return html`<${Section} title="Outputs" aside=${html`<button class="primary" onClick=${() => edit()}>+ New output</button>`}>
    ${!outputs
      ? html`<${Loading} />`
      : outputs.length === 0
        ? html`<p class="muted">Nothing yet. An output is what the automation switches: a room light with its levels, a dimmer, a relay, a roller shutter, or a group of them (“all lights”).</p>`
        : html`<div class="table-wrap"><table>
            <thead><tr><th>Name</th><th>Kind</th><th>What</th><th></th></tr></thead>
            <tbody>
              ${outputs.map(
                (o) => html`<tr key=${o.name}>
                  <td><strong>${o.name}</strong></td>
                  <td>${kindLabel(o.kind)}</td>
                  <td class="muted-cell">${details[o.name] ? summary(details[o.name], devices) : ''}</td>
                  <td class="actions">
                    <button onClick=${() => edit(o.name)}>Edit</button>
                    <button class="danger" onClick=${() => remove(o.name)}>Delete</button>
                  </td>
                </tr>`,
              )}
            </tbody>
          </table></div>`}
  <//>`;
}
