// A file chooser and upload button for firmware images.
import { useState } from 'preact/hooks';
import * as api from './api.js';
import { html, notify, Button } from './ui.js';

export function FirmwareUpload({ target, label, onDone }) {
  const [file, setFile] = useState(null);
  const [progress, setProgress] = useState(null);

  const upload = async () => {
    if (!file) return;
    setProgress(0);
    try {
      const bytes = await file.arrayBuffer();
      const result = await api.firmware.upload(target, bytes, setProgress);
      notify(result && result.restarting ? 'Firmware installed, the gateway restarts…' : 'Firmware sent');
      if (onDone) onDone(result);
    } catch (e) {
      notify(e.message, 'error');
    }
    setProgress(null);
  };

  return html`
    <div class="upload">
      <input type="file" accept=".bin" onChange=${(e) => setFile(e.target.files[0] || null)} />
      <${Button} onClick=${upload} disabled=${!file || progress !== null}>${label || 'Upload'}<//>
      ${progress !== null && html`<progress max="1" value=${progress}></progress>`}
    </div>`;
}
