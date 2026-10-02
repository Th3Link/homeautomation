import htm from 'htm';

const html = htm.bind(h);

// Hex-Konvertierung
export const decimalToHex = (d, padding = 2) => {
  const hex = Number(d).toString(16).toUpperCase();
  return hex.padStart(padding, '0');
};

// CAN-ID Formatierung
export const formatCanId = (uid, msg) => {
  return html`
    <div class="can-id-preview">
      <span style="color:#000000">${uid.substring(2, 4)}</span>
      <span style="color:#00AA00">${uid.substring(4, 6)}</span>
      <span style="color:#AA0000">${uid.substring(6, 8)}</span>
      <span style="color:#0000AA">${msg}</span>
      <span style="color:#000000">|</span>
    </div>
  `;
};
