import { useState } from 'preact/hooks';
import { apiRequest, prepareUpdate } from '../api.js';
import htm from 'htm';
import StateTab from './StateTab.js';
import DevicesTab from './DevicesTab.js';
import { DeviceProvider } from '../stores/deviceStore';

const html = htm.bind(h);

export default function App() {
  const [activeTab, setActiveTab] = useState('state');

  return html`
    <div class="app">
      <${DeviceProvider}>
        <${StateTab} />
        <${DevicesTab} />
      <//>
    </div>
  `;
}
