// stores/deviceStore.js
import { createContext } from 'preact';
import { useState, useEffect, useContext } from 'preact/hooks';
import { loadDevices } from '../api';
import htm from 'htm';

const html = htm.bind(h);
const DeviceContext = createContext();

export function DeviceProvider({ children }) {
  const [devices, setDevices] = useState([]);
  const [selected, setSelected] = useState([]);

  const refreshDevices = async () => {
    try {
      const response = await loadDevices();
      setDevices(response.devices || []);
    } catch (error) {
      console.error('Failed to load devices:', error);
      setDevices([]);
    }
  };

  const value = {
    devices,
    selected,
    setSelected,
    refreshDevices: refreshDevices,
    toggleDevice: (uid) => {
      setSelected(prev => 
        prev.includes(uid) 
          ? prev.filter(id => id !== uid) 
          : [...prev, uid]
      );
    },
    toggleAll: (uids) => {
      setSelected(prev => 
        prev.length === uids.length 
          ? [] 
          : uids
      );
    }
  };

  // Initial load
  useEffect(() => {
    refreshDevices();
  }, []);

  return html`
    <${DeviceContext.Provider} value=${value}>
      ${children}
    <//>
  `;
}

export const useDeviceStore = () => useContext(DeviceContext);
