import { render } from 'preact';
import { Device } from './pages/device.js';
import { Devices } from './pages/devices.js';
import { Gateway } from './pages/gateway.js';
import { Https } from './pages/https.js';
import { Mqtt } from './pages/mqtt.js';
import { Sensors } from './pages/sensors.js';
import { Link, match, usePath } from './router.js';
import { html, Toasts } from './ui.js';

// The paths the gateway answers with this page (src/web/assets.rs) — keep
// both lists the same.
const routes = [
  { path: '/', title: 'Devices', page: Devices },
  { path: '/devices/:device', title: 'Device', page: Device },
  { path: '/sensors', title: 'Sensors', page: Sensors },
  { path: '/mqtt', title: 'MQTT', page: Mqtt },
  { path: '/gateway', title: 'Gateway', page: Gateway },
  { path: '/https', title: 'HTTPS', page: Https },
];

const nav = [
  ['/', 'Devices'],
  ['/sensors', 'Sensors'],
  ['/mqtt', 'MQTT'],
  ['/gateway', 'Gateway'],
  ['/https', 'HTTPS'],
];

function App() {
  const path = usePath();
  const found = match(routes, path);
  const Page = found ? found.route.page : null;
  const active = (href) => (href === '/' ? path === '/' || path.startsWith('/devices') : path.startsWith(href));
  return html`
    <header class="top">
      <strong>CAN2MQTT</strong>
      <nav>${nav.map(([href, label]) => html`<${Link} href=${href} class=${active(href) ? 'active' : ''}>${label}<//>`)}</nav>
    </header>
    <main>
      ${Page ? html`<${Page} key=${path} ...${found.params} />` : html`<p class="error">No such page. <${Link} href="/">Devices<//></p>`}
    </main>
    <${Toasts} />`;
}

render(html`<${App} />`, document.getElementById('app'));
