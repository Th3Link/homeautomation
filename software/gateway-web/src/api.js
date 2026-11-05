// The gateway's REST API (gateway-core/docs/api-rest.md): one function per
// resource, so pages never build paths or bodies themselves.
import CRC32 from 'crc-32';

const BASE = '/api/v1';

export class ApiError extends Error {
  constructor(status, body) {
    super(body && body.message ? body.message : `HTTP ${status}`);
    this.status = status;
    this.code = body && body.error;
    // The request's field the error is about, if it names one.
    this.field = body && body.field;
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The gateway serves only a few connections at a time and refuses the rest,
// which looks like a network error: try those again before giving up.
// `body` is sent as JSON, or as it is when `raw` (a PEM file).
async function request(method, path, body, raw = false) {
  for (let attempt = 1; ; attempt++) {
    let response;
    try {
      response = await fetch(BASE + path, {
        method,
        headers: body === undefined ? {} : { 'Content-Type': raw ? 'application/x-pem-file' : 'application/json' },
        body: body === undefined ? undefined : raw ? body : JSON.stringify(body),
      });
    } catch (e) {
      if (attempt >= 4) throw new ApiError(0, { message: 'gateway unreachable' });
      await sleep(250 * attempt);
      continue;
    }
    const text = await response.text();
    let data = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch (e) {
      data = { message: text };
    }
    if (!response.ok) throw new ApiError(response.status, data);
    return data;
  }
}

const get = (path) => request('GET', path);
const post = (path, body) => request('POST', path, body);
const put = (path, body) => request('PUT', path, body);
const putPem = (path, pem) => request('PUT', path, pem, true);
const patch = (path, body) => request('PATCH', path, body);
const del = (path) => request('DELETE', path);
const seg = encodeURIComponent;

export const system = {
  get: () => get('/system'),
  restart: () => post('/system/restart'),
};

export const config = {
  get: () => get('/config'),
  update: (changes) => patch('/config', changes),
};

// Where the gateway is and its time zone, with today's sun times.
export const location = {
  get: () => get('/location'),
  update: (changes) => patch('/location', changes),
};

export const mqtt = {
  all: () => get('/mqtt'),
  updateAll: (changes) => patch('/mqtt', changes),
  update: (slot, changes) => patch(`/mqtt/${slot}`, changes),
  trust: (slot) => post(`/mqtt/${slot}/trust`),
  certificate: (slot) => get(`/mqtt/${slot}/certificate`),
  forget: (slot) => del(`/mqtt/${slot}/certificate`),
};

// The HTTPS certificate (ADR 0018). Uploads take the file's text (PEM).
export const tls = {
  get: () => get('/tls'),
  update: (changes) => patch('/tls', changes),
  selfSigned: (validityYears) => post('/tls/self-signed', { validity_years: validityYears }),
  csrUrl: () => `${BASE}/tls/csr`,
  installCertificate: (pem) => putPem('/tls/certificate', pem),
  setTrustAnchor: (pem) => putPem('/tls/trust-anchor', pem),
  removeTrustAnchor: () => del('/tls/trust-anchor'),
  setIdentity: (pem) => putPem('/tls/est/identity', pem),
  removeIdentity: () => del('/tls/est/identity'),
  enroll: () => post('/tls/est/enroll'),
};

export const devices = {
  // All pages of the list.
  async list() {
    const all = [];
    for (;;) {
      const page = await get(`/devices?offset=${all.length}`);
      all.push(...page.devices);
      if (page.devices.length === 0 || all.length >= page.total) return all;
    }
  },
  get: (device) => get(`/devices/${seg(device)}`),
  update: (device, changes) => patch(`/devices/${seg(device)}`, changes),
  action: (device, action) => post(`/devices/${seg(device)}/${action}`),
  silence: (device, on) => put(`/devices/${seg(device)}/silence`, { on }),
  relay: (device, number, state, durationMs, bank = 0) =>
    put(`/devices/${seg(device)}/relays/${number}`, { state, duration_ms: durationMs, bank }),
  rollershutter: (device, number, state, durationMs, bank = 0) =>
    put(`/devices/${seg(device)}/rollershutters/${number}`, { state, duration_ms: durationMs, bank }),
  lamps: (device, bank, value, bitmask) =>
    put(`/devices/${seg(device)}/lamps/${bank}`, { value, bitmask }),
  heating: (device, kind, body) => put(`/devices/${seg(device)}/heating/${kind}`, body),
  // Manual (commissioning) mode of the heating controller's relays 6-10.
  heatingManualState: (device) => get(`/devices/${seg(device)}/heating/manual`),
  heatingManual: (device, enabled, relay, on) =>
    put(`/devices/${seg(device)}/heating/manual`, relay === undefined ? { enabled } : { enabled, relay, on }),
};

export const sensors = {
  // Names a sensor and/or gives it a role; `changes` has `name` and/or `role`
  // (`none`, `tank0` … `tank6`, `flow`).
  update: (device, slot, changes) => put(`/devices/${seg(device)}/sensors/${slot}`, changes),
  // Asks the node for its sensor list (slots, names, roles).
  refresh: (device) => post(`/devices/${seg(device)}/sensors/refresh`),
  forget: (device) => post(`/devices/${seg(device)}/sensors/forget`),
  // All pages; of one device if `device` is given.
  async list(device) {
    const all = [];
    for (;;) {
      const filter = device ? `&device=${seg(device)}` : '';
      const page = await get(`/sensors?offset=${all.length}${filter}`);
      all.push(...page.sensors);
      if (page.sensors.length === 0 || all.length >= page.total) return all;
    }
  },
};

export const bus = {
  action: (action) => post(`/bus/${action}`),
  silence: (on) => put('/bus/silence', { on }),
  typeAction: (type, action) => post(`/device-types/${type}/${action}`),
};

export const firmware = {
  job: () => get('/firmware/job'),
  // `target` is `gateway`, `devices/<name>` or `device-types/<type>`.
  // `onProgress(fraction)` follows the upload to the gateway.
  upload(target, bytes, onProgress) {
    const crc = CRC32.buf(new Uint8Array(bytes)) >>> 0;
    const path = target === 'gateway' ? '/firmware/gateway' : `/${target}/firmware`;
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('PUT', `${BASE}${path}?crc32=${crc}`);
      xhr.setRequestHeader('Content-Type', 'application/octet-stream');
      xhr.upload.onprogress = (e) => e.lengthComputable && onProgress && onProgress(e.loaded / e.total);
      xhr.onload = () => {
        let body = null;
        try {
          body = JSON.parse(xhr.responseText);
        } catch (e) {}
        if (xhr.status >= 200 && xhr.status < 300) resolve(body);
        else reject(new ApiError(xhr.status, body));
      };
      xhr.onerror = () => reject(new ApiError(0, { message: 'upload failed' }));
      xhr.send(bytes);
    });
  },
};

// The latest log lines, from position `from` in the stream of everything
// logged since boot, and the switch for mirroring CAN frames to MQTT.
export const logs = {
  read: (from) => get(`/logs?from=${from}`),
  setCanToMqtt: (on) => patch('/logs', { can_to_mqtt: on }),
};

// The gateway's own automation (ADR 0025): outputs, switch configurations,
// motion lights, schedules and device notes, one item per request.
const named = (kind) => ({
  list: () => get(`/automation/${kind}`).then((r) => r.items),
  get: (name) => get(`/automation/${kind}/${seg(name)}`),
  put: (name, item) => put(`/automation/${kind}/${seg(name)}`, item),
  remove: (name) => del(`/automation/${kind}/${seg(name)}`),
});

// Every page of a paged list.
async function allPages(path) {
  const all = [];
  for (;;) {
    const page = await get(`${path}?offset=${all.length}`);
    all.push(...page.items);
    if (page.items.length === 0 || all.length >= page.total) return all;
  }
}

// A device in the automation's device routes: its key.
const deviceKey = (ref) => '0x' + (0x10000000 + (ref.type << 16) + (ref.id << 8)).toString(16);

export const automation = {
  status: () => get('/automation'),
  setEnabled: (enabled) => patch('/automation', { enabled }),
  outputs: {
    ...named('outputs'),
    // `op` as the API takes it: "on", {"set": 4}, …
    command: (name, op) => post(`/automation/outputs/${seg(name)}/command`, op),
  },
  switches: {
    // `{name, switches, buttons}` (buttons: a bit per button with actions).
    list: () => allPages('/automation/switches'),
    get: (name) => get(`/automation/switches/${seg(name)}`),
    put: (name, config) => put(`/automation/switches/${seg(name)}`, config),
    remove: (name) => del(`/automation/switches/${seg(name)}`),
    // `{events: [{event, actions}]}`; no events removes the button's.
    button: (name, button) => get(`/automation/switches/${seg(name)}/${button}`),
    putButton: (name, button, actions) => put(`/automation/switches/${seg(name)}/${button}`, actions),
  },
  motion: {
    ...named('motion'),
    setEnabled: (name, enabled) => patch(`/automation/motion/${seg(name)}`, { enabled }),
  },
  schedules: {
    ...named('schedules'),
    setEnabled: (name, enabled) => patch(`/automation/schedules/${seg(name)}`, { enabled }),
  },
  // What devices have on board, and lamp names: `{features, lamps}`.
  devices: {
    // `{device, features, lamps}` (lamps: whether it has names).
    list: () => allPages('/automation/devices'),
    get: (ref) => get(`/automation/devices/${deviceKey(ref)}`),
    put: (ref, notes) => put(`/automation/devices/${deviceKey(ref)}`, notes),
  },
};
