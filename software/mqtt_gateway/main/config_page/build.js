const esbuild = require('esbuild');

esbuild.build({
  entryPoints: ['src/js/app.js'],
  bundle: true,
  outfile: 'src/bundle.js',
  format: 'esm',
  minify: false,
  keepNames: true,
  minifyIdentifiers: false,
  treeShaking: true,
  legalComments: 'none',
  target: ['chrome58', 'firefox57', 'safari11'],
  platform: 'browser',
  jsxFactory: 'h',
  jsxFragment: 'Fragment',
  inject: ['./preact-shim.js'], // Für JSX-Unterstützung
}).then(() => console.log('Build erfolgreich!'))
  .catch(() => process.exit(1));
