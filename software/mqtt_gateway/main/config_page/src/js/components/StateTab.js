import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import { apiRequest, prepareUpdate,  completeUpdate } from '../api';

const html = htm.bind(h);

export default function StateTab() {
  const [changes, setChanges] = useState({});
  const [state, setState] = useState(null);
  const [file, setFile] = useState(null);
  const [progress, setProgress] = useState(null);
  const [updateType, setUpdateType] = useState('device');

  const handleUpload = async () => {
    if (!file) return;

    try {
      await prepareUpdate(file, 'self');

      // 2. Datei hochladen
      const formData = new FormData();
      formData.append('file', file);
      
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable) {
          setProgress(Math.round((e.loaded / e.total) * 100));
        }
      };

      await new Promise((resolve, reject) => {
        xhr.onload = resolve;
        xhr.onerror = reject;
        xhr.open('POST', '/update/data', true);
        xhr.send(formData);
      });

      // 3. Update abschließen
      await completeUpdate('self');
      setProgress(100);
      alert('Update erfolgreich!');

    } catch (error) {
      console.error('Update failed:', error);
      alert(`Update fehlgeschlagen: ${error.message}`);
    } finally {
      setProgress(null);
    }
  };
  const [isDragOver, setIsDragOver] = useState(false);
  
  const handleDragOver = (e) => {
      e.preventDefault();
      setIsDragOver(true);
  };
  
  const handleDragLeave = () => {
      setIsDragOver(false);
  };
  
  const handleDrop = (e) => {
      e.preventDefault();
      setIsDragOver(false);
      if (e.dataTransfer.files && e.dataTransfer.files[0]) {
          setFile(e.dataTransfer.files[0]);
      }
  };


  // Load state data on mount
  useEffect(() => {
    loadState();
  }, []);

  const loadState = async () => {
    const data = await apiRequest('state.json');
    setState(data);
  };

  const handleChange = (field, value, section = null) => {
    setChanges(prev => {
      const newChanges = {...prev};
      if (section) {
        newChanges[section] = {...newChanges[section], [field]: value};
      } else {
        newChanges[field] = value;
      }
      return newChanges;
    });
  };

  const handleMqttLogging = async (value) => {
    await apiRequest('/control.json', 'POST', { 
      command: "mqtt_logging", 
      enabled: value 
    });
    loadState();
  };

  const getCurrentValue = (field, section) => {
    return changes[section]?.[field]
      ?? state?.[section]?.[field]
      ?? changes[field]
      ?? state?.[field]
      ?? '';
  };

  const saveConfig = async () => {
    await apiRequest('/control.json', 'POST', { 
      command: "save_config", 
      ...changes 
    });
    loadState();
    setChanges({});

  };

  const refreshState = async () => {
    const data = await apiRequest('state.json');
    setState(data);
  };

  const restartDevice = async () => {
    await apiRequest('/control.json', 'POST', {
      command: "restart",
      unit: "self"
    });
  };

  return html`
    <div class="tab-content">

      
      <!-- General Section -->
      <div class="detail-section">
        <div class="device-config">
          <div class="config-grid">
            <div class="config-group">
              <h5>General</h5>
              ${state && html`
                <div class="status-item"><span class="status-label">Firmware Version:</span> <span class="status-value">${state.firmware_version}</span></div>
                <div class="status-item"><span class="status-label">Uptime:</span> <span class="status-value">${state.uptime}</span></div>
                <div class="status-item"><span class="status-label">Hostname:</span> <span class="status-value">${state.hostname}</span></div>
              `}
              <div class="config-row">
                <label>Hostname</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('hostname')} 
                  onInput=${e => handleChange('hostname', e.target.value)} 
                />
              </div>
                
              <div class="config-row">
                <label>Username</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('username')} 
                  onInput=${e => handleChange('username', e.target.value)}
                />
              </div>
                
              <div class="config-row">
                <label>Password</label>
                <input 
                  class="config-input"
                  type="password" 
                  value=${getCurrentValue('password')} 
                  onInput=${e => handleChange('password', e.target.value)}
                />
              </div>
                
              <div class="config-row">
                <label>Update Delay</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('update_delay') || 10} 
                  onInput=${e => handleChange('update_delay', e.target.value)}
                />
              </div>
            </div>
      
            <!-- WiFi Section -->
            <div class="config-group">
              <h5>WiFi</h5>
              ${state?.wifi && html`
                <div class="status-item"><span class="status-label">Connection State:</span> <span class="status-value">${state.wifi.state}</span></div>
                <div class="status-item"><span class="status-label">IPv4 Address:</span> <span class="status-value">${state.wifi.ipv4}</span></div>
                <div class="status-item"><span class="status-label">IPv6 Address:</span> <span class="status-value">${state.wifi.ipv6}</span></div>
                <div class="status-item"><span class="status-label">Gateway:</span> <span class="status-value">${state.wifi.gateway}</span></div>
                <div class="status-item"><span class="status-label">DNS Server:</span> <span class="status-value">${state.wifi.dns}</span></div>
              `}
              
              <div class="config-row">
                <label>Mode</label>
                <select
                  class="config-input"
                  value=${getCurrentValue('mode', 'wifi') || 'ap'} 
                  onChange=${e => handleChange('mode', e.target.value, 'wifi')}
                >
                  <option value="ap">Access Point</option>
                  <option value="client">Client</option>
                  <option value="off">Off</option>
                </select>
              </div>
                
              <div class="config-row">
                <label>SSID</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('ssid', 'wifi') || ''} 
                  onInput=${e => handleChange('ssid', e.target.value, 'wifi')}
                />
              </div>
                
              <div class="config-row">
                <label>Password</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('password', 'wifi') || ''} 
                  onInput=${e => handleChange('password', e.target.value, 'wifi')}
                />
              </div>
            </div>
      
      <!-- MQTT Section -->
            <div class="config-group">
              <h5>MQTT</h5>
              ${state?.mqtt && html`
                <div class="status-item"><span class="status-label">Connection State:</span> <span class="status-value">${state.mqtt.state}</span></div>
                <div class="status-item"><span class="status-label">Messages Received:</span> <span class="status-value">${state.mqtt.received}</span></div>
                <div class="status-item"><span class="status-label">Messages Send:</span> <span class="status-value">${state.mqtt.sent}</span></div>
              `}
              
              <div class="config-row">
                <label>Server URI</label>
                <input
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('uri', 'mqtt') || ''} 
                  onInput=${e => handleChange('uri', e.target.value, 'mqtt')}
                  placeholder="mqtt://IP:PORT"
                />
              </div>
                
              <div class="config-row">
                <label>Username</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('username', 'mqtt') || ''} 
                  onInput=${e => handleChange('username', e.target.value, 'mqtt')}
                />
              </div>
                
              <div class="config-row">
                <label>Password</label>
                <input 
                  class="config-input"
                  type="text" 
                  value=${getCurrentValue('password', 'mqtt') || ''} 
                  onInput=${e => handleChange('password', e.target.value, 'mqtt')}
                />
              </div>
            </div>
      
      <!-- CAN Bus Section -->
            <div class="config-group">
              <h5>CAN Bus</h5>
              ${state?.canbus && html`
                <div class="status-item"><span class="status-label">Messages Received:</span> <span class="status-value">${state.canbus.received}</span></div>
                <div class="status-item"><span class="status-label">Messages Send:</span> <span class="status-value">${state.canbus.sent}</span></div>
              `}
              
              <div class="config-row">
                <label>Baudrate</label>
                <select 
                  class="config-input"
                  value=${getCurrentValue('baudrate', 'canbus') || 'b50'}
                  onChange=${e => handleChange('baudrate', e.target.value, 'canbus')}
                >
                  <option value="b50">50 KBit/s</option>
                  <option value="b22_222">22.222 KBit/s</option>
                  <option value="b25">25 KBit/s</option>
                  <option value="b100">100 KBit/s</option>
                </select>
              </div>
                
              <div class="config-row">
                <label>
                  <input 
                    type="checkbox" 
                    checked=${getCurrentValue('mqtt_logging', 'canbus') || false}
                    onChange=${e => handleMqttLogging(e.target.checked)}
                  />
                  MQTT Logging
                </label>
              </div>
            </div>

            <div class="config-group">
              <h5>Firmware Update</h5>
              <div class="config-group-content">
                <div class="upload-container">
                  <label 
                      class=${`file-upload ${isDragOver ? 'drag-over' : ''}`}
                      onDragOver=${handleDragOver}
                      onDragLeave=${handleDragLeave}
                      onDrop=${handleDrop}
                  >
                    <div class="file-upload-icon">
                        <i class="fas fa-cloud-upload-alt"></i>
                    </div>
                    <div class="file-upload-text">
                        ${file ? file.name : 'Drag & Drop firmware file here'}
                    </div>
                    <div class="file-upload-hint">
                        or click to browse (.bin files only)
                    </div>
                    <input 
                        type="file" 
                        accept=".bin"
                        onChange=${e => setFile(e.target.files[0])}
                    />
                  </label>
                </div>
                ${progress !== null && html`
                  <div class="progress-container">
                    <div class="progress-bar" style=${{ width: `${progress}%` }}></div>
                    <span class="progress-text">${progress}%</span>
                  </div>
                  ${progress === 100 && html`<div class="success-message">Update completed successfully!</div>`}
                `}
              </div>
              <div class="save-button">
                <button
                  class="primary"
                  onClick=${handleUpload}
                  disabled=${!file}
                >
                    Start Update
                </button>
              </div>
            </div>
          </div>
        </div>
        <div class="save-button">
          <button class="primary" onClick=${saveConfig} disabled=${!Object.keys(changes).length}>
            Save
          </button>
          <button onClick=${refreshState}>Refresh</button>
          <button onClick=${restartDevice}>Restart</button>
        </div>
      </div>
    </div>
  `;
}
