import { h } from 'preact';
import { useState } from 'preact/hooks';
import htm from 'htm';
import { decimalToHex, formatCanId } from '../utils';
import { sendDeviceCommand } from '../api';

const html = htm.bind(h);

export default function RelaisControl({ deviceUid }) {
  const [type, setType] = useState('relais');
  const [bank, setBank] = useState(0);
  const [num, setNum] = useState(0);
  const [state, setState] = useState('off');
  const [time, setTime] = useState(0);

  const getHexCommand = () => {
    const typeHex = type === 'rollershutter' ? '83' : '82'; // 131 vs 130
    let stateHex = '00';
    if (state === 'on') stateHex = '03';
    if (state === 'up') stateHex = '01';
    if (state === 'down') stateHex = '02';
    
    const timeHex = decimalToHex(time, 6).match(/.{2}/g).reverse().join('');
    
    return {
      type: typeHex,
      num: decimalToHex(num, 2),
      state: stateHex,
      time: timeHex,
      bank: decimalToHex(bank, 2)
    };
  };

  const executeCommand = () => {
    const hex = getHexCommand();
    sendDeviceCommand(type, [deviceUid], {
      num: parseInt(num),
      state: parseInt(hex.state, 16),
      time: parseInt(time),
      bank: parseInt(bank)
    });
  };

  return html`
    <div class="config-group-content">
      <div class="control-row">
        <label>Type:</label>
        <select class="config-input" value=${type} onChange=${e => setType(e.target.value)}>
          <option value="relais">Relais</option>
          <option value="rollershutter">Rollershutter</option>
        </select>
      </div>

      <div class="control-row">
        <label>Bank:</label>
        <input class="config-input" type="number" min="0" max="255" value=${bank} 
          onInput=${e => setBank(e.target.value)} />
      </div>

      <div class="control-row">
        <label>Number:</label>
        <input class="config-input" type="number" min="0" max="255" value=${num} 
          onInput=${e => setNum(e.target.value)} />
      </div>

      <div class="control-row">
        <label>State:</label>
        <select class="config-input" value=${state} onChange=${e => setState(e.target.value)}>
          ${type === 'relais' ? html`
            <option value="off">OFF</option>
            <option value="on">ON</option>
          ` : html`
            <option value="off">OFF</option>
            <option value="up">UP</option>
            <option value="down">DOWN</option>
          `}
        </select>
      </div>

      <div class="control-row">
        <label>Time (ms):</label>
        <input class="config-input" type="number" min="0" value=${time} 
          onInput=${e => setTime(e.target.value)} />
      </div>

      <div class="can-preview">
        ${formatCanId(deviceUid, getHexCommand().type)}
        <span style="color:#0000AA">${getHexCommand().num}</span>
        <span style="color:#00AA00">${getHexCommand().state}</span>
        <span style="color:#AA0000">${getHexCommand().time}</span>
        <span style="color:#0000AA">${getHexCommand().bank}</span>
      </div>
    </div>
    <div class="save-button">
      <button class="primary" onClick=${executeCommand}>Execute</button>
    </div>
  `;
}
