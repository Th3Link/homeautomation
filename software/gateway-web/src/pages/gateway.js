import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { FirmwareUpload } from '../firmware-upload.js';
import { html, useLoad, run, notify, duration, Section, Loading, Field, Button } from '../ui.js';

export function Gateway() {
  return html`<${Info} /><${Settings} /><${Firmware} />`;
}

function Info() {
  const { data, error } = useLoad(api.system.get, 5000);
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
          </dl>`}
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
