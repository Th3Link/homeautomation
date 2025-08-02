import { h } from 'preact';
import htm from 'htm';
import { useState } from 'preact/hooks';

const html = htm.bind(h);

export default function DeviceConfig({ device, onSave }) {
  const [config, setConfig] = useState({
    rollershutter_mode: device.rollershutter_mode,
    device_id: device.device_id,
    device_type: device.device_type,
    hwrev: device.hwrev,
    legacy_sensor: device.legacy_sensor,
    custom_string: device.custom_string,
    baudrate: device.baudrate
  });

  return html`
    <div class="device-status">
      <h4>Status</h4>
      <div class="status-grid">
        <div class="status-item"><span class="status-label">Firmware Version:</span> <span class="status-value">${device.version}</span></div>
        <div class="status-item"><span class="status-label">HW Revision:</span> <span class="status-value">${device.hwrev}</span></div>
        <div class="status-item"><span class="status-label">Device UID0:</span> <span class="status-value">${device.uid0}</span></div>
        <div class="status-item"><span class="status-label">Device UID1:</span> <span class="status-value">${device.uid1}</span></div>
        <div class="status-item"><span class="status-label">Baudrate:</span> <span class="status-value">${device.baudrate}</span></div>
        <div class="status-item"><span class="status-label">Uptime:</span> <span class="status-value">${device.uptime}</span></div>
      </div>
    </div>
  `;
}
