import { h } from 'preact';
import htm from 'htm';
import { sendDeviceCommand } from '../api';
import { useDeviceStore } from '../stores/deviceStore';

const html = htm.bind(h);

export default function BatchControls() {
  const { devices, selected, refreshDevices } = useDeviceStore();
  const handleCommand = async (command) => {
    try {
      await sendDeviceCommand(command, selected);
      refreshDevices();
    } catch (error) {
      alert(`Error: ${error.message}`);
    }
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
    </div>
  `;
}
