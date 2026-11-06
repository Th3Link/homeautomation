import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { FirmwareUpload } from '../firmware-upload.js';
import { html, useLoad, run, notify, duration, Section, Loading, Field, Button } from '../ui.js';

export function Gateway() {
  // One request for both sections: the gateway serves one HTTPS session
  // at a time.
  const system = useLoad(api.system.get, 5000);
  return html`<${Info} ...${system} /><${Network} ...${system} /><${NetworkSettings} /><${Settings} /><${Location} /><${Firmware} />`;
}

function Info({ data, error }) {
  const restart = async () => {
    if (!confirm('Restart the gateway?')) return;
    await run(api.system.restart, 'Restarting…');
  };
  return html`
    <${Section} title="Gateway" aside=${html`<${Button} kind="danger" onClick=${restart}>Restart<//>`}>
      ${!data
        ? html`<${Loading} error=${error} />`
        : html`<dl class="facts">
            <div><dt>Firmware</dt><dd>${data.firmware_version}</dd></div>
            <div><dt>Uptime</dt><dd>${duration(data.uptime_s)}</dd></div>
            <div><dt>Heap</dt><dd>${Math.round(data.heap_used / 1024)} KiB used, ${Math.round(data.heap_free / 1024)} KiB free</dd></div>
            <div><dt>Stack</dt><dd>${Math.round(data.stack_untouched / 1024)} of ${Math.round(data.stack_size / 1024)} KiB never used</dd></div>
          </dl>`}
    <//>`;
}

function Network({ data, error }) {
  if (!data) return html`<${Section} title="Network"><${Loading} error=${error} /><//>`;
  const n = data.network;
  if (!n) return html`<${Section} title="Network"><p>No Ethernet.</p><//>`;
  const list = (items) => items.map((i) => html`<div>${i.address} <span class="muted">(${i.origin})</span></div>`);
  const dhcpv6 =
    n.dhcpv6_setting === 'off' ? 'switched off'
    : n.dhcpv6_setting === 'auto' && !n.slaac ? 'off (follows the routers, which SLAAC reads)'
    : n.dhcpv6_setting === 'enforce' ? 'enforced: addresses and configuration'
    : { off: 'not offered by the routers', stateless: 'configuration only (O)', stateful: 'addresses and configuration (M)' }[n.dhcpv6_mode];
  return html`
    <${Section} title="Network" aside=${html`<span class=${'badge ' + (n.link_up ? 'ok' : 'warn')}>${n.link_up ? 'link up' : 'link down'}</span>`}>
      <dl class="facts">
        <div><dt>IPv6</dt><dd>${n.ipv6.length ? list(n.ipv6) : 'none'}</dd></div>
        <div><dt>IPv6 gateway</dt><dd>${n.ipv6_gateway || 'none'}</dd></div>
        <div><dt>DHCPv6</dt><dd>${dhcpv6}</dd></div>
        <div><dt>IPv4</dt><dd>${n.ipv4 ? html`${n.ipv4} <span class="muted">(${n.ipv4_origin})</span>` : n.ipv4_origin === 'DHCP' ? 'no DHCP lease' : 'none'}</dd></div>
        <div><dt>IPv4 gateway</dt><dd>${n.ipv4_gateway || 'none'}</dd></div>
        <div><dt>DNS servers</dt><dd>${n.dns_servers.length ? list(n.dns_servers) : 'none'}</dd></div>
        <div><dt>MAC</dt><dd>${n.mac}</dd></div>
      </dl>
    <//>`;
}

// `address/prefix` in a field, or empty.
const cidr = (address, prefix) => (address ? `${address}/${prefix}` : '');
function parseCidr(text, defaultPrefix) {
  const [address, prefix] = text.trim().split('/');
  return { address: address || null, prefix: prefix ? +prefix : defaultPrefix };
}

const toForm = (s) => ({
  ipv4_mode: s.ipv4_mode,
  ipv4: cidr(s.ipv4_address, s.ipv4_prefix),
  ipv4_gateway: s.ipv4_gateway || '',
  dns1: s.dns_servers[0] || '',
  dns2: s.dns_servers[1] || '',
  slaac: s.slaac,
  dhcpv6: s.dhcpv6,
  ipv6: cidr(s.ipv6_address, s.ipv6_prefix),
  ipv6_gateway: s.ipv6_gateway || '',
});

function fromForm(f) {
  const v4 = parseCidr(f.ipv4, 24);
  const v6 = parseCidr(f.ipv6, 64);
  return {
    ipv4_mode: f.ipv4_mode,
    ipv4_address: f.ipv4_mode === 'manual' ? v4.address : null,
    ipv4_prefix: v4.prefix,
    ipv4_gateway: f.ipv4_mode === 'manual' ? f.ipv4_gateway.trim() || null : null,
    dns_servers: [f.dns1, f.dns2].map((d) => d.trim()).filter(Boolean),
    slaac: f.slaac,
    dhcpv6: f.dhcpv6,
    ipv6_address: v6.address,
    ipv6_prefix: v6.prefix,
    ipv6_gateway: f.ipv6_gateway.trim() || null,
  };
}

function NetworkSettings() {
  const { data, error, reload } = useLoad(api.network.get);
  const [form, setForm] = useState(null);
  const title = 'Network settings';
  if (!data) return html`<${Section} title=${title}><${Loading} error=${error} /><//>`;
  const f = form || toForm(data.settings);
  const set = (key) => (e) => setForm({ ...f, [key]: e.target.type === 'checkbox' ? e.target.checked : e.target.value });
  const manual = f.ipv4_mode === 'manual';

  const save = async () => {
    if (!form) return notify('Nothing changed');
    if (!confirm('Store the network settings? They take effect after a restart. If the gateway is unreachable then, the serial console\'s network-reset brings back DHCP and SLAAC.')) return;
    if (await run(() => api.network.put(fromForm(form)), 'Saved')) {
      setForm(null);
      reload();
    }
  };

  return html`
    <${Section} title=${title} aside=${data.restart_required ? html`<span class="badge warn">restart to apply</span>` : null}>
      <h3>IPv4</h3>
      <div class="row">
        <${Field} label="Address"><select value=${f.ipv4_mode} onChange=${set('ipv4_mode')}>
          <option value="dhcp">DHCP</option>
          <option value="link_local">Link-local (${data.link_local_ipv4})</option>
          <option value="manual">Manual</option>
        </select><//>
        ${manual && html`
          <${Field} label="Address/prefix" hint="e.g. 192.168.1.20/24"><input value=${f.ipv4} onInput=${set('ipv4')} /><//>
          <${Field} label="Gateway" hint="empty: the local network only"><input value=${f.ipv4_gateway} onInput=${set('ipv4_gateway')} /><//>`}
      </div>
      <div class="row">
        <${Field} label="DNS server 1" hint=${manual ? 'at least one' : 'optional: replaces the DHCP server\'s'}><input value=${f.dns1} onInput=${set('dns1')} /><//>
        <${Field} label="DNS server 2"><input value=${f.dns2} onInput=${set('dns2')} /><//>
      </div>
      <h3>IPv6</h3>
      <div class="row">
        <label class="check"><input type="checkbox" checked=${f.slaac} onChange=${set('slaac')} /> SLAAC (addresses and router from the routers' advertisements)</label>
        <${Field} label="DHCPv6"><select value=${f.dhcpv6} onChange=${set('dhcpv6')}>
          <option value="auto">As the routers ask (M/O flags)</option>
          <option value="enforce">Always (address and configuration)</option>
          <option value="off">Off</option>
        </select><//>
      </div>
      ${!f.slaac && f.dhcpv6 === 'auto' && html`<p class="muted">Without SLAAC the routers' flags aren't read, so DHCPv6 only runs when enforced.</p>`}
      <div class="row">
        <${Field} label="Address/prefix" hint="optional, next to the others, e.g. 2001:db8::20/64"><input value=${f.ipv6} onInput=${set('ipv6')} /><//>
        <${Field} label="Gateway" hint=${f.slaac ? 'only used without SLAAC' : 'e.g. fe80::1'}><input value=${f.ipv6_gateway} onInput=${set('ipv6_gateway')} /><//>
      </div>
      <div class="buttons"><${Button} kind="primary" onClick=${save}>Save<//>${form && html`<button onClick=${() => setForm(null)}>Discard</button>`}</div>
    <//>`;
}

function Settings() {
  const { data, error, reload } = useLoad(api.config.get);
  const [form, setForm] = useState({});
  if (!data) return html`<${Loading} error=${error} />`;
  const value = (key) => (key in form ? form[key] : data[key]);
  const set = (key, number) => (e) => setForm({ ...form, [key]: number ? +e.target.value : e.target.value });

  const save = async () => {
    const changes = { ...form };
    if (!changes.web_password) delete changes.web_password;
    if (Object.keys(changes).length === 0) return notify('Nothing changed');
    const result = await run(() => api.config.update(changes), 'Saved');
    if (result) {
      if (result.restart_required) notify('Restart the gateway to apply the hostname or bitrate.');
      setForm({});
      reload();
    }
  };

  return html`
    <${Section} title="Settings">
      <div class="row">
        <${Field} label="Hostname"><input value=${value('hostname')} onInput=${set('hostname')} /><//>
        <${Field} label="CAN bitrate"><select value=${value('can_bitrate')} onChange=${set('can_bitrate')}>
          ${['b22_222', 'b25', 'b50', 'b100'].map((b) => html`<option>${b}</option>`)}
        </select><//>
        <${Field} label="CAN update delay (ms)" hint="between frames while updating a device"><input type="number" min="1" max="10000" value=${value('update_delay_ms')} onInput=${set('update_delay_ms', true)} /><//>
      </div>
      <h3>Web access</h3>
      <div class="row">
        <${Field} label="User name"><input value=${value('web_username')} onInput=${set('web_username')} autocomplete="off" /><//>
        <${Field} label="New password" hint="at least 8 characters; empty keeps it"><input type="password" value=${form.web_password || ''} onInput=${set('web_password')} autocomplete="new-password" /><//>
      </div>
      <div class="buttons"><${Button} kind="primary" onClick=${save}>Save<//></div>
    <//>`;
}

const SUN = [
  ['astronomical_dawn', 'Astronomical dawn'],
  ['nautical_dawn', 'Nautical dawn'],
  ['civil_dawn', 'Civil dawn'],
  ['sunrise', 'Sunrise'],
  ['noon', 'Solar noon'],
  ['sunset', 'Sunset'],
  ['civil_dusk', 'Civil dusk'],
  ['nautical_dusk', 'Nautical dusk'],
  ['astronomical_dusk', 'Astronomical dusk'],
];

function Location() {
  const { data, error, reload } = useLoad(api.location.get);
  const [form, setForm] = useState({});
  if (!data) return html`<${Section} title="Location and time"><${Loading} error=${error} /><//>`;
  const value = (key) => (key in form ? form[key] : data[key] ?? '');
  const set = (key) => (e) => setForm({ ...form, [key]: e.target.value });

  const save = async () => {
    const changes = {};
    if ('latitude' in form || 'longitude' in form) {
      changes.latitude = parseFloat(value('latitude'));
      changes.longitude = parseFloat(value('longitude'));
      if (Number.isNaN(changes.latitude) || Number.isNaN(changes.longitude)) return notify('Latitude and longitude are numbers');
    }
    if ('timezone' in form) changes.timezone = form.timezone.trim();
    if (Object.keys(changes).length === 0) return notify('Nothing changed');
    if (await run(() => api.location.update(changes), 'Saved')) {
      setForm({});
      reload();
    }
  };

  // The browser knows where it is (HTTPS, with the gateway's certificate
  // trusted); it asks first.
  const locate = () => {
    if (!navigator.geolocation) return notify('This browser has no location', 'error');
    navigator.geolocation.getCurrentPosition(
      (p) => setForm({ ...form, latitude: p.coords.latitude.toFixed(4), longitude: p.coords.longitude.toFixed(4) }),
      (e) => notify(`No location: ${e.message}`, 'error'),
      { timeout: 15000 },
    );
  };

  const offset = data.utc_offset_minutes;
  const zone = offset == null ? '' : `UTC${offset < 0 ? '−' : '+'}${Math.floor(Math.abs(offset) / 60)}${Math.abs(offset) % 60 ? ':' + String(Math.abs(offset) % 60).padStart(2, '0') : ''}`;
  return html`
    <${Section} title="Location and time">
      <p class="muted">
        For switching by local time and by the sun (schedules, motion light levels). Two decimals (about 1 km) are plenty.
        Take this browser's location, or look the place up on a map: in OpenStreetMap or Google Maps a right click on the house shows
        latitude and longitude. Save, then check today's sunrise and sunset below.
      </p>
      <div class="row">
        <${Field} label="Latitude" hint="degrees, north positive"><input inputmode="decimal" value=${value('latitude')} onInput=${set('latitude')} /><//>
        <${Field} label="Longitude" hint="degrees, east positive"><input inputmode="decimal" value=${value('longitude')} onInput=${set('longitude')} /><//>
        <${Field} label="Time zone" hint="POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3"><input value=${value('timezone')} onInput=${set('timezone')} /><//>
      </div>
      <div class="buttons"><${Button} kind="primary" onClick=${save}>Save<//><button onClick=${locate}>Use this browser's location</button></div>
      <dl class="facts">
        <div><dt>Local time</dt><dd>${data.local_time ? `${data.local_time.replace('T', ' ')} (${zone})` : 'not known yet (no time server answer)'}</dd></div>
        ${data.sun
          ? SUN.map(([key, label]) => html`<div><dt>${label}</dt><dd>${data.sun[key] || html`<span class="muted">not today</span>`}</dd></div>`)
          : html`<div><dt>Sun times</dt><dd class="muted">${data.latitude == null ? 'set the location first' : 'once the time is known'}</dd></div>`}
      </dl>
    <//>`;
}

function Firmware() {
  return html`
    <${Section} title="Gateway firmware">
      <p class="muted">Installs a new firmware image (the .bin of gateway-hardware) and restarts into it.</p>
      <${FirmwareUpload} target="gateway" label="Install and restart" />
    <//>`;
}
