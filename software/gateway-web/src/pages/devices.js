import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { Link } from '../router.js';
import { html, useLoad, run, ago, hex, Section, Loading, Button } from '../ui.js';

export function Devices() {
  const { data, error, reload } = useLoad(api.devices.list, 5000);
  const [uid, setUid] = useState('');

  // Sends a bus command, then looks at the list again once the devices had
  // time to answer.
  const bus = (call, done) => async () => {
    await run(call, done);
    setTimeout(reload, 1500);
  };

  const pingUid = async () => {
    const id = uid.trim();
    if (!id) return;
    await run(() => api.devices.action(id, 'ping'), `Ping sent to ${id}`);
  };

  const restartAll = async () => {
    if (!confirm('Restart every device on the bus?')) return;
    await bus(() => api.bus.action('restart'), 'Restart sent to every device')();
  };

  // Makes every device announce itself and its settings, which also finds
  // the ones that aren't in the list yet.
  const scan = bus(async () => {
    await api.bus.action('ping');
    await api.bus.action('refresh');
  }, 'Scanning: every device was asked to announce itself');

  return html`
    <${Section} title="Devices">
      <div class="toolbar">
        <${Button} onClick=${reload}>Refresh<//>
        <${Button} onClick=${bus(() => api.bus.action('ping'), 'Ping sent to every device')}>Broadcast ping<//>
        <${Button} onClick=${bus(() => api.bus.silence(true), 'Every device is silenced')}>Silence on<//>
        <${Button} onClick=${bus(() => api.bus.silence(false), 'Every device may talk again')}>Silence off<//>
        <${Button} kind="danger" onClick=${restartAll}>Restart all<//>
        <input class="uid" placeholder="Name or 0x… key" value=${uid} onInput=${(e) => setUid(e.target.value)} onKeyDown=${(e) => e.key === 'Enter' && pingUid()} />
        <${Button} onClick=${pingUid}>Ping<//>
        <${Button} onClick=${scan}>Scan<//>
      </div>
      ${!data
        ? html`<${Loading} error=${error} />`
        : data.length === 0
          ? html`<p class="muted">No device seen on the bus yet. Try a scan.</p>`
          : html`<div class="table-wrap"><table>
              <thead><tr><th>Name</th><th>ID</th><th>Type</th><th>Version</th><th>Seen</th><th>Uptime</th><th>State</th></tr></thead>
              <tbody>
                ${data.map(
                  (d) => html`<tr key=${d.key}>
                    <td><${Link} href=${'/devices/' + encodeURIComponent(d.name || d.key)}>${d.name || d.key}<//></td>
                    <td><code>${hex(d.device_id)}</code></td>
                    <td>${d.type_name} <code>${hex(d.device_type)}</code></td>
                    <td>${d.version}</td>
                    <td>${ago(d.last_seen_minutes_ago)}</td>
                    <td>${d.uptime ? ago(Math.floor(d.uptime / 60)) : ''}</td>
                    <td>${d.error ? html`<span class="badge bad">error ${d.error}</span>` : d.last_seen_minutes_ago > 10 ? html`<span class="badge warn">silent</span>` : html`<span class="badge ok">ok</span>`}</td>
                  </tr>`,
                )}
              </tbody>
            </table></div>`}
      ${error && data && html`<p class="error">${error.message}</p>`}
    <//>`;
}
