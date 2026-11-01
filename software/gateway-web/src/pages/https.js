// HTTPS / Certificates: the web server's certificate, from one place
// (gateway-hardware src/cert, ADR 0018).
import { useState } from 'preact/hooks';
import * as api from '../api.js';
import { html, useLoad, run, notify, Section, Loading, Field, Button } from '../ui.js';

const MODES = [
  ['self_signed', 'Self-signed', 'The gateway signs its own certificate. Needs nothing else; browsers warn until you trust it.'],
  ['manual', 'Manual certificate', 'Download the CSR, have it signed by your CA, upload the certificate.'],
  ['est', 'EST automatic', 'The gateway enrolls and renews over EST (RFC 7030) with your CA.'],
];

const STATUS = {
  valid: ['Valid', 'ok'],
  expires_soon: ['Expires soon', 'warn'],
  expired: ['Expired', 'bad'],
  not_yet_valid: ['Not yet valid', 'bad'],
  fqdn_mismatch: ['Not for this FQDN', 'warn'],
  time_unknown: ['Valid (time not synchronised yet)', 'muted'],
};

const ORIGIN = {
  self_signed: 'self-signed',
  self_signed_fallback: 'self-signed (fallback: there was no usable certificate)',
  manual: 'uploaded',
  est: 'EST',
};

// Like `run`, but true on success: these routes answer 204 without a body.
const done = async (call, success) => (await run(call, success)) !== undefined;

const date = (text) => (text ? new Date(text).toLocaleString() : '—');

// Reads a chosen file as text and hands it to `send`.
function FileButton({ label, accept, send, success, after }) {
  const choose = async (e) => {
    const file = e.target.files[0];
    e.target.value = '';
    if (!file) return;
    const text = await file.text();
    if ((await done(() => send(text), success)) && after) after();
  };
  return html`<label class="button file">${label}<input type="file" accept=${accept || '.pem,.crt,.cer'} onChange=${choose} hidden /></label>`;
}

export function Https() {
  const { data, error, reload } = useLoad(api.tls.get, 10000);
  if (!data) return html`<${Loading} error=${error} />`;
  return html`
    <${Certificate} d=${data} />
    <${Settings} d=${data} reload=${reload} />
    ${data.mode === 'self_signed' && html`<${SelfSigned} d=${data} reload=${reload} />`}
    ${data.mode === 'manual' && html`<${Manual} reload=${reload} />`}
    ${data.mode === 'est' && html`<${Est} d=${data} reload=${reload} />`}
    <${TrustAnchor} d=${data} reload=${reload} />
    <${Time} d=${data} reload=${reload} />`;
}

function CertFacts({ c }) {
  return html`<dl class="facts">
    <div><dt>Subject</dt><dd>${c.subject || '—'}</dd></div>
    <div><dt>Issuer</dt><dd>${c.issuer || '—'}</dd></div>
    <div><dt>Valid from</dt><dd>${date(c.not_before)}</dd></div>
    <div><dt>Valid until</dt><dd>${date(c.not_after)}</dd></div>
    ${c.dns_names && c.dns_names.length > 0 && html`<div><dt>SAN</dt><dd>${c.dns_names.map((n) => 'DNS:' + n).join(', ')}</dd></div>`}
    <div><dt>SHA-256</dt><dd><code>${c.fingerprint_sha256}</code></dd></div>
  </dl>`;
}

function Certificate({ d }) {
  const c = d.certificate;
  const [label, kind] = c ? STATUS[c.status] || [c.status, 'muted'] : ['none', 'bad'];
  return html`
    <${Section} title="Certificate" aside=${html`<span class=${'badge ' + kind}>${label}</span>`}>
      ${c
        ? html`<${CertFacts} c=${c} />
            <p class="muted">Made: ${ORIGIN[c.origin] || c.origin}; ${c.count} certificate${c.count === 1 ? '' : 's'} in the chain.</p>`
        : html`<p class="error">No certificate. The gateway makes a self-signed one at the next start.</p>`}
      ${d.key_pin_sha256 && html`<dl class="facts">
        <div><dt>Key pin</dt><dd><code>${d.key_pin_sha256}</code></dd></div>
      </dl>
      <p class="muted">The pin-sha256 (RFC 7469) of the gateway's own key, the same for every certificate and CSR. An EST server that registers clients by their key wants this.</p>`}
      ${!d.https && html`<p class="error">HTTPS is switched off (console: web-https on, then restart).</p>`}
    <//>`;
}

function Settings({ d, reload }) {
  const [fqdn, setFqdn] = useState(d.fqdn);
  const [mode, setMode] = useState(d.mode);
  const save = async () => {
    const changes = {};
    if (fqdn !== d.fqdn) changes.fqdn = fqdn.trim();
    if (mode !== d.mode) changes.mode = mode;
    if (Object.keys(changes).length === 0) return notify('Nothing changed');
    if (await done(() => api.tls.update(changes), 'Saved')) reload();
  };
  return html`
    <${Section} title="HTTPS settings">
      <div class="row">
        <${Field} label="FQDN" hint=${'the name the gateway is reached by; empty: ' + d.effective_fqdn}>
          <input value=${fqdn} placeholder=${d.effective_fqdn} onInput=${(e) => setFqdn(e.target.value)} />
        <//>
      </div>
      <h3>Certificate mode</h3>
      ${MODES.map(([value, label, hint]) => html`
        <label class="choice">
          <input type="radio" name="mode" value=${value} checked=${mode === value} onChange=${() => setMode(value)} />
          <span><strong>${label}</strong><small class="muted">${hint}</small></span>
        </label>`)}
      <p class="muted">Changing the FQDN in self-signed mode makes a new certificate right away; in the other modes the current one stays until the new one is there.</p>
      <div class="buttons"><${Button} kind="primary" onClick=${save}>Save<//></div>
    <//>`;
}

function SelfSigned({ d, reload }) {
  const [years, setYears] = useState(d.validity_years);
  const generate = async () => {
    if (!confirm('Make a new self-signed certificate? Browsers that trusted the old one will warn again.')) return;
    if (await done(() => api.tls.selfSigned(years), 'New certificate made; reload the page to use it')) reload();
  };
  return html`
    <${Section} title="Self-signed certificate">
      <div class="row">
        <${Field} label="Validity (years)" hint="1-30"><input type="number" min="1" max="30" value=${years} onInput=${(e) => setYears(+e.target.value)} /><//>
      </div>
      <div class="buttons"><${Button} onClick=${generate}>Generate / regenerate<//></div>
    <//>`;
}

function Manual({ reload }) {
  return html`
    <${Section} title="Manual certificate">
      <p class="muted">The request is for the gateway's own key, which never leaves it. Upload the issued certificate with its chain (PEM); it replaces the current one only if it is for this key and FQDN.</p>
      <div class="buttons">
        <a class="button" href=${api.tls.csrUrl()} download="gateway.csr">Download CSR</a>
        <${FileButton} label="Upload certificate" send=${api.tls.installCertificate} success="Certificate installed; reload the page to use it" after=${reload} />
      </div>
    <//>`;
}

function Est({ d, reload }) {
  const e = d.est;
  const [url, setUrl] = useState(e.url);
  const [auto, setAuto] = useState(e.auto_renew);
  const save = async () => {
    if (await done(() => api.tls.update({ est_url: url.trim(), est_auto_renew: auto }), 'Saved')) reload();
  };
  const enroll = async () => {
    if (await done(api.tls.enroll, 'Enrollment requested')) setTimeout(reload, 3000);
  };
  return html`
    <${Section} title="EST">
      <div class="row">
        <${Field} label="EST server" hint="https://host[:port][/.well-known/est/label]; an IPv6 address in brackets: https://[fd00::1]"><input value=${url} onInput=${(ev) => setUrl(ev.target.value)} /><//>
      </div>
      <label class="check"><input type="checkbox" checked=${auto} onChange=${(ev) => setAuto(ev.target.checked)} /> Renew automatically (30 days before expiry)</label>
      <p>Trust anchor: ${d.trust_anchor ? html`<span class="badge ok">installed</span>` : html`<span class="badge bad">missing</span> (below)`}
        ${!d.time_known && html` · <span class="badge warn">waiting for the time (NTP)</span>`}</p>
      <dl class="facts">
        <div><dt>Last attempt</dt><dd>${date(e.last_attempt)}</dd></div>
        <div><dt>Result</dt><dd>${e.last_result || '—'}${e.last_status ? ` (HTTP ${e.last_status})` : ''}</dd></div>
        ${e.next_attempt_in_s != null && html`<div><dt>Next attempt</dt><dd>in ${e.next_attempt_in_s} s</dd></div>`}
      </dl>
      <h3>Device identity (mTLS)</h3>
      <p class="muted">If the EST server wants a client certificate for the first enrollment: one PEM file with the certificate and its private key. The key can't be read back. Renewals authenticate with the current certificate.</p>
      ${e.identity && html`<${CertFacts} c=${e.identity} />`}
      <div class="buttons">
        <${FileButton} label=${e.identity ? 'Replace identity' : 'Install identity'} accept=".pem" send=${api.tls.setIdentity} success="Identity installed" after=${reload} />
        ${e.identity && html`<${Button} kind="danger" onClick=${async () => (await done(api.tls.removeIdentity, 'Identity removed')) && reload()}>Remove<//>`}
      </div>
      <div class="buttons">
        <${Button} kind="primary" onClick=${save}>Save<//>
        <${Button} onClick=${enroll}>Enroll / renew now<//>
      </div>
    <//>`;
}

function TrustAnchor({ d, reload }) {
  const t = d.trust_anchor;
  const builtIn = async (on) => {
    if (await done(() => api.tls.update({ built_in_roots: on }), on ? 'Built-in CAs trusted' : 'Built-in CAs no longer trusted')) reload();
  };
  return html`
    <${Section} title="Trusted certificate authorities">
      <p class="muted">The EST server's certificate has to chain to one of these, and so has that of an MQTT broker (mqtts://) whose key isn't pinned.</p>
      <label class="check"><input type="checkbox" checked=${d.built_in_roots} onChange=${(e) => builtIn(e.target.checked)} /> Built in: Let's Encrypt (${d.built_in_root_names.join(', ')})</label>
      <h3>Installed</h3>
      ${t ? html`<${CertFacts} c=${t} />${t.count > 1 && html`<p class="muted">${t.count} CA certificates installed; the first is shown.</p>`}` : html`<p>None installed.</p>`}
      <div class="buttons">
        <${FileButton} label=${t ? 'Replace' : 'Upload'} send=${api.tls.setTrustAnchor} success="CA certificates installed" after=${reload} />
        ${t && html`<${Button} kind="danger" onClick=${async () => confirm('Remove the installed CA certificates?') && (await done(api.tls.removeTrustAnchor, 'Removed')) && reload()}>Remove<//>`}
      </div>
    <//>`;
}

function Time({ d, reload }) {
  const [server, setServer] = useState(d.ntp_server);
  const save = async () => {
    if (await done(() => api.tls.update({ ntp_server: server.trim() }), 'Saved; used at the next synchronisation')) reload();
  };
  return html`
    <${Section} title="Time" aside=${html`<span class=${'badge ' + (d.time_known ? 'ok' : 'warn')}>${d.time_known ? 'synchronised' : 'not synchronised'}</span>`}>
      <p class="muted">Certificate dates, EST and MQTT brokers checked against a CA need the time; the gateway has no clock of its own.</p>
      <div class="row">
        <${Field} label="NTP server"><input value=${server} placeholder="automatic" onInput=${(e) => setServer(e.target.value)} /><//>
      </div>
      <p class="muted">Empty: the server DHCPv6 names, else pool.ntp.org. In use: ${d.ntp_server_in_use}</p>
      <div class="buttons"><${Button} onClick=${save}>Save<//></div>
    <//>`;
}
