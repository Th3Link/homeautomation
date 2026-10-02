import DeviceRow from './DeviceRow';
import { useDeviceStore } from '../stores/deviceStore';
import htm from 'htm';

const html = htm.bind(h);

export default function DeviceTable() {
  const { devices, selected, toggleDevice, toggleAll, refreshDevices } = useDeviceStore();

  return html`
    <table id="device_table">
      <thead>
        <tr class="header">
          <th><input type="checkbox" onChange=${() => toggleAll(devices.map(d => d.uid))} checked=${selected.length === devices.length} /></th>
          <th>Unique ID</th>
          <th>Device ID</th>
          <th>Type</th>
          <th>Type Name</th>
          <th>Custom String</th>
          <th>Last Seen</th>
          <th>State</th>
          <th>Error</th>
        </tr>
      </thead>
      <tbody id="device_table_body">
        ${devices.map((device, index) => html`
          <${DeviceRow} 
            device=${device}
            selected=${selected.includes(device.uid)}
            onToggle=${() => toggleDevice(device.uid)}
            rowIndex=${index}
          />
        `)}
      </tbody>
    </table>
  `;
}
