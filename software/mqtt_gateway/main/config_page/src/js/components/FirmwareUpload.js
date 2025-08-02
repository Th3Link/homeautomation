import { h } from 'preact';
import htm from 'htm';
import { useState } from 'preact/hooks';
import { prepareUpdate,  completeUpdate } from '../api';
import { useDeviceStore } from '../stores/deviceStore';

const html = htm.bind(h);

export default function FirmwareUpload({ deviceUid }) {
  const { refreshDevices } = useDeviceStore();
  const [file, setFile] = useState(null);
  const [progress, setProgress] = useState(null);
  const [updateType, setUpdateType] = useState('device'); // 'device' or 'self'

  const handleUpload = async () => {
    if (!file) return;

    try {
      await prepareUpdate(file, 'can_by_uid', deviceUid);

      // 2. Datei hochladen
      const formData = new FormData();
      formData.append('file', file);
      
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable) {
          setProgress(Math.round((e.loaded / e.total) * 100));
        }
      };

      await new Promise((resolve, reject) => {
        xhr.onload = resolve;
        xhr.onerror = reject;
        xhr.open('POST', '/update/data', true);
        xhr.send(formData);
      });

      // 3. Update abschließen
      await completeUpdate('can_by_uid', deviceUid);
      setProgress(100);
      refreshDevices();
      alert('Update erfolgreich!');

    } catch (error) {
      console.error('Update failed:', error);
      alert(`Update fehlgeschlagen: ${error.message}`);
    } finally {
      setProgress(null);
    }
  };
  const [isDragOver, setIsDragOver] = useState(false);
  
  const handleDragOver = (e) => {
      e.preventDefault();
      setIsDragOver(true);
  };
  
  const handleDragLeave = () => {
      setIsDragOver(false);
  };
  
  const handleDrop = (e) => {
      e.preventDefault();
      setIsDragOver(false);
      if (e.dataTransfer.files && e.dataTransfer.files[0]) {
          setFile(e.dataTransfer.files[0]);
      }
  };
  return html`
    <div class="config-group-content">
      <div class="upload-container">
        <label 
            class=${`file-upload ${isDragOver ? 'drag-over' : ''}`}
            onDragOver=${handleDragOver}
            onDragLeave=${handleDragLeave}
            onDrop=${handleDrop}
        >
          <div class="file-upload-icon">
              <i class="fas fa-cloud-upload-alt"></i>
          </div>
          <div class="file-upload-text">
              ${file ? file.name : 'Drag & Drop firmware file here'}
          </div>
          <div class="file-upload-hint">
              or click to browse (.bin files only)
          </div>
          <input 
              type="file" 
              accept=".bin"
              onChange=${e => setFile(e.target.files[0])}
          />
        </label>
      </div>
      ${progress !== null && html`
        <div class="progress-container">
          <div class="progress-bar" style=${{ width: `${progress}%` }}></div>
          <span class="progress-text">${progress}%</span>
        </div>
        ${progress === 100 && html`<div class="success-message">Update completed successfully!</div>`}
      `}
    </div>
    <div class="save-button">
      <button
        class="primary"
        onClick=${handleUpload}
        disabled=${!file}
      >
          Start Update
      </button>
    </div>
  `;
}
