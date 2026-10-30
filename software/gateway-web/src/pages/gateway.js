import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { FirmwareUpload } from '../firmware-upload.js';
import { html, useLoad, run, notify, duration, Section, Loading, Field, Button } from '../ui.js';

export function Gateway() {
  // One request for both sections: the gateway serves one HTTPS session
  // at a time.
  const system = useLoad(api.system.get, 5000);
  return html`<${Info} ...${system} /><${Network} ...${system} /><${Settings} /><${Firmware} />`;
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
  const dhcpv6 = { off: 'not offered by the routers', stateless: 'configuration only (O)', stateful: 'addresses and configuration (M)' }[n.dhcpv6_mode];
  return html`
    <${Section} title="Network" aside=${html`<span class=${'badge ' + (n.link_up ? 'ok' : 'warn')}>${n.link_up ? 'link up' : 'link down'}</span>`}>
      <dl class="facts">
        <div><dt>IPv6</dt><dd>${n.ipv6.length ? list(n.ipv6) : 'none'}</dd></div>
        <div><dt>IPv6 gateway</dt><dd>${n.ipv6_gateway || 'none'}</dd></div>
        <div><dt>DHCPv6</dt><dd>${dhcpv6}</dd></div>
        <div><dt>IPv4</dt><dd>${n.ipv4 ? html`${n.ipv4} <span class="muted">(DHCP)</span>` : 'no DHCP lease'}</dd></div>
        <div><dt>IPv4 gateway</dt><dd>${n.ipv4_gateway || 'none'}</dd></div>
        <div><dt>DNS servers</dt><dd>${n.dns_servers.length ? list(n.dns_servers) : 'none'}</dd></div>
        <div><dt>MAC</dt><dd>${n.mac}</dd></div>
      </dl>
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

function Firmware() {
  return html`
    <${Section} title="Gateway firmware">
      <p class="muted">Installs a new firmware image (the .bin of gateway-hardware) and restarts into it.</p>
      <${FirmwareUpload} target="gateway" label="Install and restart" />
    <//>`;
}
