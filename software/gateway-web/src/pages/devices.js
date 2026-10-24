import * as api from '../api.js';
import { Link } from '../router.js';
import { html, useLoad, run, ago, Section, Loading, Button } from '../ui.js';

export function Devices() {
  const { data, error, reload } = useLoad(api.devices.list, 5000);

  const broadcast = (action) => async () => {
    await run(() => api.bus.action(action), `${action} sent to every device`);
    setTimeout(reload, 1500);
  };

  return html`
    <${Section}
      title="Devices"
      aside=${html`<span class="buttons">
        <${Button} onClick=${broadcast('ping')}>Ping all<//>
        <${Button} onClick=${broadcast('refresh')}>Refresh all<//>
      </span>`}
    >
      ${!data
        ? html`<${Loading} error=${error} />`
        : data.length === 0
          ? html`<p class="muted">No device seen on the bus yet.</p>`
          : html`<div class="table-wrap"><table>
              <thead><tr><th>Name</th><th>Type</th><th>ID</th><th>Version</th><th>Seen</th><th>Uptime</th><th>State</th></tr></thead>
              <tbody>
                ${data.map(
                  (d) => html`<tr key=${d.key}>
                    <td><${Link} href=${'/devices/' + encodeURIComponent(d.name || d.key)}>${d.name || d.key}<//></td>
                    <td>${d.type_name}</td>
                    <td>${d.device_id}</td>
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
