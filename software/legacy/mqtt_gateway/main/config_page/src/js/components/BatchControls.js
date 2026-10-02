import { h } from 'preact';
import htm from 'htm';
import { sendDeviceCommand } from '../api';
import { useDeviceStore } from '../stores/deviceStore';
import { useState } from 'preact/hooks';

const html = htm.bind(h);

export default function BatchControls() {
  const { devices, selected, refreshDevices } = useDeviceStore();
  const [pingUid, setPingUid] = useState("");
  const handleCommand = async (command) => {
    try {
      await sendDeviceCommand(command, selected);
      refreshDevices();
    } catch (error) {
      alert(`Error: ${error.message}`);
    }
  };

  const handlePingUid = () => {
    if (!pingUid.trim()) {
      alert("Bitte UID eingeben!");
      return;
    }
    console.log("pingUid", [pingUid.trim()])
    sendDeviceCommand("ping", [pingUid.trim()]);
  };
  return html`
    <div class="batch-controls">
      <button onClick=${refreshDevices}>Refresh All</button>
      <button onClick=${() => handleCommand('ping')}>
        ${selected.length ? 'Ping Selected' : 'Broadcast Ping'}
      </button>
      <button onClick=${() => handleCommand('silence_on')}>Silence On</button>
      <button onClick=${() => handleCommand('silence_off')}>Silence Off</button>
      <button onClick=${() => handleCommand('restart')}>
        ${selected.length ? 'Restart Selected' : 'Restart All'}
      </button>
      <input
        type="text"
        value=${pingUid}
        onInput=${e => setPingUid(e.target.value)}
        placeholder="Device UID eingeben"
        class="config-input"
      />
      <button onClick=${handlePingUid}>Ping UID</button>
      <button onClick=${() => handleCommand('scan')}>Scan</button>
    </div>
  `;
}
