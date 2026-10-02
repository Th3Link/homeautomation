import * as CRC32 from 'crc-32';

/**
 * Central API Service for Device Management
 * Handles all communication with the ESP32 backend
 */

/**
 * Fetches current device state from server
 * @returns {Promise<{devices: Array, header: Array}>}
 */
export const loadDevices = async () => {
  const response = await fetch('state.json');
  if (!response.ok) throw new Error(`HTTP ${response.status} - Failed to load devices`);
  return await response.json();
};

/**
 * Sends a command to one or more devices
 * @param {string} command - The command to execute
 * @param {string[]} [deviceIds=[]] - Array of device UIDs
 * @param {Object} [params={}] - Additional command parameters
 * @returns {Promise<void>}
 */
export const sendDeviceCommand = async (command, deviceIds = [], params = {}) => {
  let unit = "can_all";
  let commandId;

  if (deviceIds.length === 1) {
    unit = "can_by_uid";
    commandId = deviceIds[0];
  } else if (deviceIds.length > 1) {
    unit = "can_selected";
    commandId = deviceIds;
  }
  const payload = {
    command,
    ...params,
    unit,
    ...(commandId !== undefined && { commandId })
  };

  const response = await fetch('/control.json', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload)
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status} - Command failed`);
  }
};

/**
 * Prepares a firmware update
 * @param {File} file - The firmware file
 * @param {string} [targetType='can_by_uid'] - 'self' or 'can_by_uid' or 'can_by_type'
 * @param {string|null} [targetId=null] - Target UID or type
 * @returns {Promise<{update_size: number, update_crc: string}>}
 */
export const prepareUpdate = async (file, targetType = 'can_by_uid', targetId = null) => {
  const fileBuffer = await file.arrayBuffer();
  const crc32Signed = CRC32.buf(new Uint8Array(fileBuffer));
  const crc32Unsigned = crc32Signed >>> 0;
  
  const payload = {
    command: "update_prepare",
    update_type: targetType,
    update_size: file.size.toString(),
    update_crc: crc32Unsigned.toString()
  };

  if (targetId) {
    payload.update_id = targetId;
  }

  const response = await fetch('/control.json', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload)
  });

  if (!response.ok) throw new Error('Prepare update failed');
  return payload;
};

/**
 * Complete a firmware update
 * @param {string} [targetType='can_by_uid'] - 'self' or 'can_by_uid' or 'can_by_type'
 * @param {string|null} [targetId=null] - Target UID or type
 * @returns {Promise<{update_size: number, update_crc: string}>}
 */
export const completeUpdate = async (targetType = 'can_by_uid', targetId = null) => {
  const payload = {
    command: "update_complete",
    update_type: targetType,
  };

  if (targetId) {
    payload.update_id = targetId;
  }

  const response = await fetch('/control.json', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload)
  });

  if (!response.ok) throw new Error('Prepare update failed');
  return payload;
};
/**
 * Saves device configuration
 * @param {string} deviceId - Device UID
 * @param {Object} config - Configuration changes
 * @returns {Promise<void>}
 */
export const saveDeviceConfig = async (deviceId, config) => {
  await sendDeviceCommand('save_config', [deviceId], config);
};

/**
 * Special commands with additional parameters
 */
export const deviceCommands = {
  relayControl: (deviceId, { type, bank, num, state, time }) => 
    sendDeviceCommand(type, [deviceId], { bank, num, state, time }),
  
  lampControl: (deviceId, { value, bank, bitmask, instant }) =>
    sendDeviceCommand('lamps', [deviceId], { value, bank, bitmask, instant }),
  
  broadcastPing: () => sendDeviceCommand('ping'),
  
  factoryReset: (deviceId) => 
    sendDeviceCommand('factory_reset', [deviceId])
};

export const apiRequest = async (endpoint, method = 'GET', data = null) => {
  try {
    const options = { method, headers: { 'Content-Type': 'application/json' } };
    if (data) options.body = JSON.stringify(data);
    
    const response = await fetch(endpoint, options);
    return await response.json();
  } catch (error) {
    console.error('API Error:', error);
    return null;
  }
};



const executeRelayCommand = (uid) => {
  const type = document.getElementById(`${uid}_relais_type`).value;
  const bank = document.getElementById(`${uid}_relais_bank`).value;
  const num = document.getElementById(`${uid}_relais_num`).value;
  const state = document.getElementById(`${uid}_relais_state`).value;
  const time = document.getElementById(`${uid}_input_relais_time`).value;

  return apiRequest('/control.json', 'POST', {
    command: type,
    unit: "can_by_uid",
    commandId: uid,
    bank: parseInt(bank),
    num: parseInt(num),
    state,
    time: parseInt(time)
  });
};

const executeLampsCommand = (uid) => {
  const value = document.getElementById(`${uid}_input_lamps_value`).value;
  const bank = document.getElementById(`${uid}_lamps_bank`).value;
  const instant = document.getElementById(`${uid}_input_lamps_instant`).checked;
  
  let bitmask = 0;
  for (let i = 0; i < 24; i++) {
    if (document.getElementById(`${uid}_input_lamps_bitmask_${i}`).checked) {
      bitmask |= (1 << (23 - i));
    }
  }

  return apiRequest('/control.json', 'POST', {
    command: "lamps",
    unit: "can_by_uid",
    commandId: uid,
    value: parseInt(value),
    bitmask,
    bank: parseInt(bank),
    instant
  });
};

const updateDeviceConfig = (uid, field, value) => {
  return apiRequest('/control.json', 'POST', {
    command: "save",
    unit: "can_by_uid",
    commandId: uid,
    [field]: value
  });
};
