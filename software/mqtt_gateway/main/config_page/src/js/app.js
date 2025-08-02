import { h, render } from 'preact';
import htm from 'htm';
import { crc32 } from 'crc-32';
import { apiRequest, prepareUpdate } from './api.js';

import App from './components/App.js';
const html = htm.bind(h);
render(html`<${App} />`, document.getElementById('app'));
