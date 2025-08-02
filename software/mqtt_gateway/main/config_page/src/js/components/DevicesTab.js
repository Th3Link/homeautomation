import { h } from 'preact';
import htm from 'htm';
import DeviceTable from './DeviceTable';
import BatchControls from './BatchControls';
import { loadDevices } from '../api';
import { useDeviceStore } from '../stores/deviceStore';

const html = htm.bind(h);

export default function DevicesTab() {

  const { devices, selected, refreshDevices } = useDeviceStore();

  return html`
    <div class="tab-content">
      <${BatchControls} />
      <${DeviceTable} />
    </div>
  `;
}
