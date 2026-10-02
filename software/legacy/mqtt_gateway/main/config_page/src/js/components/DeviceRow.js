import { h } from 'preact';
import { useState } from 'preact/hooks';
import htm from 'htm';
import DeviceDetails from './DeviceDetails';

const html = htm.bind(h);

export default function DeviceRow({ device, selected, onToggle, rowIndex }) {
  const [expanded, setExpanded] = useState(false);
  return html`
    <tr id=${device.uid} class="base-row ${rowIndex % 2 === 0 ? 'even' : 'odd'}" onClick=${() => setExpanded(!expanded)}>
      <td><input type="checkbox" checked=${selected} onClick=${e => {
        e.stopPropagation();
        onToggle(device.uid);
      }} /></td>
      <td>${device.uid}</td>
      <td>${device.device_id}</td>
      <td>${device.device_type}</td>
      <td>${device.device_type_name}</td>
      <td>${device.custom_string}</td>
      <td>${device.last_seen}</td>
      <td>${device.state}</td>
      <td>${device.last_error}</td>
    </tr>
    
    ${expanded && html`
      <tr class="details ${rowIndex % 2 === 0 ? 'even' : 'odd'}">
        <td colspan="9">
          <${DeviceDetails} device=${device} />
        </td>
      </tr>
    `}
  `;
}
