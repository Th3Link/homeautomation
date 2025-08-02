import { h } from 'preact';
import { useState, useRef } from 'preact/hooks';
import htm from 'htm';
import { decimalToHex, formatCanId } from '../utils';
import { sendDeviceCommand } from '../api';

const html = htm.bind(h);

export default function PwmControl({ deviceUid }) {
  const [value, setValue] = useState(0);
  const [bank, setBank] = useState(0);
  const [bitmask, setBitmask] = useState(0);
  const [instant, setInstant] = useState(false);
  const [lastSelected, setLastSelected] = useState(null);
  const containerRef = useRef();

  const toggleBit = (bit, isShiftSelect = false, forceState = null) => {
    setBitmask(prev => {
      let newMask = prev;
      
      if (isShiftSelect && lastSelected !== null) {
        const start = Math.min(lastSelected, bit);
        const end = Math.max(lastSelected, bit);
        const referenceState = (prev & (1 << (23 - lastSelected))) !== 0;
        
        for (let i = start; i <= end; i++) {
          if (referenceState) {
            newMask |= (1 << (23 - i));
          } else {
            newMask &= ~(1 << (23 - i));
          }
        }
      } else if (forceState !== null) {
        // Expliziten Zustand setzen
        if (forceState) {
          newMask |= (1 << (23 - bit));
        } else {
          newMask &= ~(1 << (23 - bit));
        }
      } else {
        // Normales Toggeln
        newMask = prev ^ (1 << (23 - bit));
      }
      
      return newMask;
    });
    
    setLastSelected(bit);
  };
  
  const handleMouseDown = (bit, e) => {
    // Nur für linke Maustaste
    if (e.button !== 0) return;
    
    // Zustand des angeklickten Bits merken
    const initialBitState = (bitmask & (1 << (23 - bit))) !== 0;
    
    // Bei Shift-Klick anders behandeln
    if (e.shiftKey && lastSelected !== null) {
      toggleBit(bit, true);
    } else {
      // Normaler Klick: Toggeln
      toggleBit(bit, false);
      
      // Für Drag-Vorgang
      const handleMouseMove = (e) => {
        const checkboxElements = containerRef.current.querySelectorAll('.bitmask-checkbox');
        const currentCheckbox = document.elementFromPoint(e.clientX, e.clientY);
        const index = Array.from(checkboxElements).indexOf(currentCheckbox);
        
        if (index >= 0 && index <= 23) {
          toggleBit(index, false, !initialBitState);
        }
      };
  
      const handleMouseUp = () => {
        window.removeEventListener('mousemove', handleMouseMove);
        window.removeEventListener('mouseup', handleMouseUp);
      };
  
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }
  };
  
  const handleClick = (bit, e) => {
    // Verhindern, dass MouseDown + MouseUp als zusätzlicher Click zählt
    e.preventDefault();
  };

  const getHexCommand = () => {
    const bitmaskHex = decimalToHex(bitmask, 6).match(/.{2}/g).join('');
    return {
      cmd: '5A',
      value: decimalToHex(value, 2),
      bitmask: bitmaskHex,
      bank: decimalToHex(bank, 2)
    };
  };

  const executeCommand = () => {
    const hex = getHexCommand();
    sendDeviceCommand('lamps', [deviceUid], {
      value: parseInt(value),
      bitmask: parseInt(bitmask),
      bank: parseInt(bank)
    });
  };

  return html`
    <div class="config-group-content">
      <div class="control-row">
        <label>Value (0-255):</label>
        <input type="range" min="0" max="255" value=${value} 
          onInput=${e => setValue(e.target.value)} />
        <span>${value}</span>
      </div>

      <div class="control-row">
        <label>Bank:</label>
        <input type="number" min="0" max="255" value=${bank} 
          onInput=${e => setBank(e.target.value)} />
      </div>

      <div class="bitmask-container" ref=${containerRef}>
        <div class="bitmask-header">
          ${Array.from({ length: 24 }).map((_, i) => html`
            <span class="bitmask-number">${23-i}</span>
          `)}
        </div>
        <div class="bitmask-grid">
          ${Array.from({ length: 24 }).map((_, i) => html`
            <input
              type="checkbox"
              class="bitmask-checkbox"
              checked=${(bitmask & (1 << (23 - i))) !== 0}
              onMouseDown=${e => handleMouseDown(i, e)}
              onClick=${e => handleClick(i, e)}
            />
          `)}
        </div>
      </div>

      <div class="can-preview">
        ${formatCanId(deviceUid, getHexCommand().cmd)}
        <span style="color:#0000AA">${getHexCommand().value}</span>
        <span style="color:#00AA00">${getHexCommand().bitmask}</span>
        <span style="color:#0000AA">${getHexCommand().bank}</span>
      </div>
    </div>
    <div class="save-button">
      <button class="primary" onClick=${executeCommand}>Execute</button>
    </div>
  `;
}
