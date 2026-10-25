// Builds the UI into dist/: three gzip'd files that gateway-hardware embeds
// (include_bytes! in src/web/assets.rs). dist/ is checked in, so building the
// firmware doesn't need node; run `npm run build` after changing src/.
const esbuild = require('esbuild');
const fs = require('fs');
const zlib = require('zlib');

const gzip = (name, data) => {
  fs.mkdirSync('dist', { recursive: true });
  const packed = zlib.gzipSync(data, { level: 9 });
  fs.writeFileSync(`dist/${name}.gz`, packed);
  console.log(`dist/${name}.gz: ${data.length} -> ${packed.length} bytes`);
};

esbuild
  .build({
    entryPoints: ['src/main.js'],
    bundle: true,
    write: false,
    format: 'esm',
    minify: true,
    legalComments: 'none',
    target: ['es2020'],
    platform: 'browser',
  })
  .then((result) => {
    gzip('app.js', result.outputFiles[0].contents);
    gzip('index.html', fs.readFileSync('src/index.html'));
    gzip('style.css', fs.readFileSync('src/style.css'));
  })
  .catch(() => process.exit(1));
