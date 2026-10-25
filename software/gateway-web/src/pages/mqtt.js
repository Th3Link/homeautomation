import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { html, useLoad, run, notify, Section, Loading, Field, Button } from '../ui.js';

const ROLES = {
  primary: 'Everything the gateway publishes goes here.',
  secondary: 'Only listens (commands come in); the gateway publishes nothing to it.',
};

export function Mqtt() {
  const { data, error, reload } = useLoad(api.mqtt.all, 5000);
  if (!data) return html`<${Loading} error=${error} />`;
  return html`
    <${Broker} slot="primary" b=${data.primary} reload=${reload} />
    <${Broker} slot="secondary" b=${data.secondary} reload=${reload} />`;
}

function Broker({ slot, b, reload }) {
  const [form, setForm] = useState({ uri: b.uri, user: b.user, password: '', enabled: b.enabled, tls_insecure: b.tls_insecure });
  const [certificate, setCertificate] = useState(null);
  const tls = form.uri.startsWith('mqtts://');
  const set = (key) => (e) => setForm({ ...form, [key]: e.target.type === 'checkbox' ? e.target.checked : e.target.value });

  const save = async () => {
    const changes = { uri: form.uri, user: form.user, enabled: form.enabled, tls_insecure: form.tls_insecure };
    if (form.password) changes.password = form.password;
    if (await run(() => api.mqtt.update(slot, changes), `${slot} broker saved, reconnecting`)) {
      setForm({ ...form, password: '' });
      setTimeout(reload, 2500);
    }
  };

  const showCertificate = async () => {
    const c = await run(() => api.mqtt.certificate(slot));
    if (c) setCertificate(c);
  };

  return html`
    <${Section}
      title=${slot[0].toUpperCase() + slot.slice(1) + ' broker'}
      aside=${html`<span class=${'badge ' + (b.connected ? 'ok' : b.enabled ? 'warn' : 'muted')}>${b.connected ? 'connected' : b.enabled ? 'not connected' : 'off'}</span>`}>
      <p class="muted">${ROLES[slot]}</p>
      <div class="row">
        <${Field} label="Address" hint="mqtt://host[:1883] or mqtts://host[:8883]"><input value=${form.uri} onInput=${set('uri')} /><//>
        <${Field} label="User"><input value=${form.user} onInput=${set('user')} autocomplete="off" /><//>
        <${Field} label="Password" hint=${b.password_set ? 'set; empty keeps it' : 'none set'}><input type="password" value=${form.password} onInput=${set('password')} autocomplete="new-password" /><//>
      </div>
      <label class="check"><input type="checkbox" checked=${form.enabled} onChange=${set('enabled')} /> Connect to this broker</label>
      ${tls && html`
        <div class="tls">
          <h3>Encryption</h3>
          <p class="muted">The broker's certificate is pinned: after "Trust", only a broker with the same key is accepted.</p>
          <label class="check"><input type="checkbox" checked=${form.tls_insecure} onChange=${set('tls_insecure')} /> Accept any certificate (encrypted, but not authenticated)</label>
          <div class="buttons">
            <${Button} onClick=${async () => { await run(() => api.mqtt.trust(slot), 'The next connection pins the broker\'s certificate'); setTimeout(() => { reload(); showCertificate(); }, 4000); }}>Trust the broker's certificate now<//>
            <${Button} onClick=${showCertificate}>Show pinned certificate<//>
            ${b.certificate_pinned && html`<${Button} kind="danger" onClick=${async () => { await run(() => api.mqtt.forget(slot), 'Certificate forgotten'); setCertificate(null); reload(); }}>Forget<//>`}
          </div>
          ${certificate && html`<p>${certificate.pinned ? html`SHA-256 <code>${certificate.sha256}</code>` : 'No certificate pinned.'}</p>`}
        </div>`}
      <div class="buttons"><${Button} kind="primary" onClick=${save}>Save<//></div>
    <//>`;
}
