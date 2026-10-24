import * as api from '../api.js';
import { Link } from '../router.js';
import { html, useLoad, ago, Section, Loading } from '../ui.js';

const KINDS = { temperature: 'Temperature', humidity: 'Humidity', pressure: 'Pressure', co2: 'CO₂', voc: 'VOC', air_quality: 'Air quality' };

// The table of sensors; of one device if `device` is given, else of all.
export function SensorTable({ device }) {
  const { data, error } = useLoad(() => api.sensors.list(device), 5000);
  if (!data) return html`<${Loading} error=${error} />`;
  if (data.length === 0) return html`<p class="muted">No sensor reading seen yet.</p>`;
  return html`<div class="table-wrap"><table>
    <thead><tr>${!device && html`<th>Device</th>`}<th>Kind</th><th>Sensor</th><th>Value</th><th>Seen</th></tr></thead>
    <tbody>
      ${data.map(
        (s) => html`<tr key=${s.device_key + s.kind + s.sensor_id}>
          ${!device && html`<td><${Link} href=${'/devices/' + encodeURIComponent(s.device)}>${s.device}<//></td>`}
          <td>${KINDS[s.kind] || s.kind}</td>
          <td><code>${s.sensor_id}</code></td>
          <td>${s.kind === 'temperature' ? s.value.toFixed(2) : s.value} ${s.unit}</td>
          <td>${s.age_s < 60 ? `${s.age_s} s` : ago(Math.floor(s.age_s / 60))}</td>
        </tr>`,
      )}
    </tbody>
  </table></div>`;
}

export function Sensors() {
  return html`<${Section} title="Sensors"><${SensorTable} /><//>`;
}
