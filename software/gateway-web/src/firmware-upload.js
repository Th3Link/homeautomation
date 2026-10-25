// A file chooser and upload button for firmware images. `target` is where the
// image goes (see api.firmware.upload); `others` are more targets
// ({ name, target }) for a second button that sends the same image to all of
// them one after the other (the gateway acknowledges per device, so there is
// no broadcast).
import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from './api.js';
import { html, notify, Button } from './ui.js';

export function FirmwareUpload({ target, label, others = [], onDone }) {
  const [file, setFile] = useState(null);
  const [progress, setProgress] = useState(null);
  const [status, setStatus] = useState('');
  const [job, setJob] = useState(null);
  const timer = useRef(null);

  useEffect(() => () => clearInterval(timer.current), []);

  // While an image goes to a device over the bus, the gateway reports how far
  // the device has acknowledged it, and how often blocks had to be repeated.
  const followJob = () => {
    clearInterval(timer.current);
    timer.current = setInterval(async () => {
      try {
        setJob(await api.firmware.job());
      } catch (e) {}
    }, 1500);
  };

  const send = async (name, to) => {
    setStatus(name);
    setProgress(0);
    if (to !== 'gateway') followJob();
    try {
      const bytes = await file.arrayBuffer();
      const result = await api.firmware.upload(to, bytes, setProgress);
      return result || {};
    } finally {
      clearInterval(timer.current);
      setProgress(null);
    }
  };

  const run = async (targets) => {
    if (!file) return;
    let done = 0;
    for (const { name, target: to } of targets) {
      try {
        const result = await send(name, to);
        done += 1;
        if (result.restarting) notify('Firmware installed, the gateway restarts…');
      } catch (e) {
        notify(`${name}: ${e.message}`, 'error');
        break;
      }
    }
    if (done === targets.length && targets[0].target !== 'gateway') {
      notify(done === 1 ? 'Firmware sent; the device restarts into it' : `Firmware sent to ${done} devices`);
    }
    setStatus('');
    setJob(null);
    if (onDone) onDone();
  };

  const busy = progress !== null;
  const progressText = job && job.state === 'running'
    ? `${job.sent} of ${job.total} bytes acknowledged${job.protocol ? ` (${job.protocol})` : ''}${job.retries ? `, ${job.retries} blocks repeated` : ''}`
    : '';

  return html`
    <div class="upload">
      <input type="file" accept=".bin" onChange=${(e) => setFile(e.target.files[0] || null)} />
      <${Button} onClick=${() => run([{ name: label || 'device', target }])} disabled=${!file || busy}>${label || 'Upload'}<//>
      ${others.length > 0 && html`<${Button} onClick=${() => run([{ name: 'this device', target }, ...others])} disabled=${!file || busy}>Send to all ${others.length + 1} of this type<//>`}
      ${busy && html`<progress max="1" value=${progress}></progress>`}
      ${busy && html`<p class="muted wide">${status}${progressText ? ' — ' + progressText : ''}</p>`}
    </div>`;
}
