import { h } from 'preact';
import htm from 'htm';
import { useState } from 'preact/hooks';
import RelaisControl from './RelaisControl';
import PwmControl from './PwmControl';
import FirmwareUpload from './FirmwareUpload';
import { sendDeviceCommand } from '../api';
import { useDeviceStore } from '../stores/deviceStore';

const html = htm.bind(h);

export default function DeviceDetails({ device }) {
  const [config, setConfig] = useState({
    rollershutter_mode: device.rollershutter_mode,
    device_id: device.device_id,
    device_type: device.device_type,
    hwrev: device.hwrev,
    legacy_sensor: device.legacy_sensor,
    custom_string: device.custom_string,
    baudrate: device.baudrate,
    uid0: device.uid0,
    uid1: device.uid1
  });

  const { refreshDevices } = useDeviceStore();

  const handleCommand = async (command) => {
    try {
      await sendDeviceCommand(command, device.uid);
        refreshDevices();
    } catch (error) {
      alert(`Error: ${command} failed - ${error.message}`);
    }
  };
  const handleSaveConfig = async (uid, config) => {
    try {
      await sendDeviceCommand('save', uid, config);
      refreshDevices();
    } catch (error) {
      alert('Save failed: ' + error.message);
    }
  };

  const handleChange = (field, value) => {
    setConfig(prev => ({
      ...prev,
      [field]: field === 'hwrev' 
        ? parseInt(value) || 0 
        : value
    }));
  };
  return html`
    <div class="device-details">
      <div class="detail-section">
        <div class="device-config">
          <div class="config-grid">
            <div class="config-group">
              <h5>Status</h5>
              <div class="status-item"><span class="status-label">Firmware Version:</span> <span class="status-value">${device.version}</span></div>
              <div class="status-item"><span class="status-label">HW Revision:</span> <span class="status-value">${device.hwrev}</span></div>
              <div class="status-item"><span class="status-label">Device UID0:</span> <span class="status-value">${device.uid0}</span></div>
              <div class="status-item"><span class="status-label">Device UID1:</span> <span class="status-value">${device.uid1}</span></div>
              <div class="status-item"><span class="status-label">Baudrate:</span> <span class="status-value">${device.baudrate}</span></div>
              <div class="status-item"><span class="status-label">Uptime:</span> <span class="status-value">${device.uptime}</span></div>
            </div>
    
            <div class="config-group">
              <h5>Basic Settings</h5>
              <div class="config-row">
                <label>Device ID</label>
                <input type="text" value=${config.device_id} 
                  onChange=${e => handleChange('device_id', e.target.value)}
                  class="config-input small" />
              </div>
              
              <div class="config-row">
                <label>Device Type</label>
                <input type="text" value=${config.device_type} 
                  onChange=${e => handleChange('device_type', e.target.value)}
                  class="config-input small" />
              </div>
              
              <div class="config-row">
                <label>HW Rev</label>
                <input type="text" value=${config.hwrev} 
                  onChange=${e => handleChange('hwrev', e.target.value)}
                  class="config-input small" />
              </div>
              
              <div class="config-row">
                <label>Custom String</label>
                <input type="text" value=${config.custom_string} 
                  onChange=${e => handleChange('custom_string', e.target.value)}
                  maxlength="8" class="config-input" />
              </div>
      
              <div class="config-row">
                <label>Baudrate</label>
                <select
                  value=${config.baudrate}
                  onChange=${e => handleChange('baudrate', e.target.value)}
                  class="config-input"
                >
                  <option value="b50">50 KBit/s</option>
                  <option value="b22_222">22.222 KBit/s</option>
                  <option value="b25">25 KBit/s</option>
                  <option value="b100">100 KBit/s</option>
                </select>
              </div>
            </div>
      
            <div class="config-group">
              <h5>Device Behavior</h5>
              <div class="config-row">
                <label>Relais Mode</label>
                <select 
                  value=${device.relais_mode} 
                  onChange=${e => handleChange('relais_mode', e.target.value)}
                  class="config-input"
                >
                  <option value="OFF" selected=${device.relais_mode === 'OFF'}>Off</option>
                  <option value="RELAIS" selected=${device.relais_mode === 'RELAIS'}>Relais</option>
                  <option value="SWROLLERSHUTTER" selected=${device.relais_mode === 'SWROLLERSHUTTER'}>Software Rollershutter</option>
                  <option value="HWROLLERSHUTTER" selected=${device.relais_mode === 'HWROLLERSHUTTER'}>Hardware Rollershutter</option>
                </select>
              </div>
      
              <div class="config-row">
                <label>Extension Mode</label>
                <select 
                  value=${device.extension_mode} 
                  onChange=${e => handleChange('extension_mode', e.target.value)}
                  class="config-input"
                >
                  <option value="OFF" selected=${device.extension_mode === 'OFF'}>Off</option>
                  <option value="BUTTONS" selected=${device.extension_mode === 'BUTTONS'}>Buttons</option>
                  <option value="RELAIS" selected=${device.extension_mode === 'RELAIS'}>Relais</option>
                  <option value="SWROLLERSHUTTER" selected=${device.extension_mode === 'SWROLLERSHUTTER'}>Software Rollershutter</option>
                  <option value="HWROLLERSHUTTER" selected=${device.extension_mode === 'HWROLLERSHUTTER'}>Hardware Rollershutter</option>
                  <option value="PWM" selected=${device.extension_mode === 'PWM'}>PWM</option>
                  <option value="SENSORS" selected=${device.extension_mode === 'SENSORS'}>Sensors</option>
                  <option value="LEGACY_SENSORS" selected=${device.extension_mode === 'LEGACY_SENSORS'}>Legacy Sensors</option>
                </select>
              </div>
            </div>
          </div>
          <div class="save-button">
            <button class="primary" onClick=${() => handleSaveConfig(device.uid, config)}>Save All Changes</button>
            <button onClick=${() => handleCommand('refresh')}>Refresh</button>
            <button onClick=${() => handleCommand('ping')}>Ping</button>
            <button onClick=${() => handleCommand('restart')}>Restart</button>
            <button onClick=${() => handleCommand('silence_on')}>Silence On</button>
            <button onClick=${() => handleCommand('silence_off')}>Silence Off</button>
            <button onClick=${() => handleCommand('legacy_mode')}>Legacy Mode</button>
          </div>
        </div>
      </div>
      <div class="detail-section">
        <div class="device-config">
          <div class="config-grid">
            <div class="config-group">
              <h5>Firmware Update</h5>
              <${FirmwareUpload} deviceUid=${device.uid} />
            </div>
            <div class="config-group">
              <h5>Relais Control</h5>
              <${RelaisControl} deviceUid=${device.uid} />
            </div>
            <div class="config-group">
              <h5>PWM Control</h5>
              <${PwmControl} deviceUid=${device.uid} />
            </div>
          </div>
        </div>
      </div>

    </div>
  `;
}
