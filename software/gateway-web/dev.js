// Serves src/ with the API proxied to a real gateway, for working on the UI:
//   GATEWAY=192.168.178.128 USER=CAN2MQTTSETUP PASSWORD=... npm run dev
const http = require('http');
const esbuild = require('esbuild');
const fs = require('fs');

const gateway = process.env.GATEWAY;
if (!gateway) {
  console.error('set GATEWAY to the gateway address');
  process.exit(1);
}
const auth = 'Basic ' + Buffer.from(`${process.env.USER_NAME || 'CAN2MQTTSETUP'}:${process.env.PASSWORD || 'Can2MqttPass'}`).toString('base64');

http
  .createServer(async (req, res) => {
    if (req.url.startsWith('/api/')) {
      const body = await new Promise((r) => {
        const chunks = [];
        req.on('data', (c) => chunks.push(c));
        req.on('end', () => r(Buffer.concat(chunks)));
      });
      const proxied = http.request(
        { host: gateway, path: req.url, method: req.method, headers: { ...req.headers, host: gateway, authorization: auth } },
        (p) => {
          res.writeHead(p.statusCode, p.headers);
          p.pipe(res);
        },
      );
      proxied.on('error', () => { res.writeHead(502); res.end('gateway unreachable'); });
      proxied.end(body);
    } else if (req.url === '/assets/app.js') {
      const out = await esbuild.build({ entryPoints: ['src/main.js'], bundle: true, write: false, format: 'esm', sourcemap: 'inline' });
      res.writeHead(200, { 'Content-Type': 'text/javascript' });
      res.end(out.outputFiles[0].contents);
    } else if (req.url === '/assets/style.css') {
      res.writeHead(200, { 'Content-Type': 'text/css' });
      res.end(fs.readFileSync('src/style.css'));
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(fs.readFileSync('src/index.html'));
    }
  })
  .listen(8080, () => console.log('http://localhost:8080/'));
