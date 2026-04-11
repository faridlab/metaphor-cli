#!/usr/bin/env node
// Downloads the platform-specific metaphor binary from GitHub releases
// into ./dist/ during `npm install`.

const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');

const pkg = require('./package.json');
const REPO = 'faridlab/metaphor-cli';
const VERSION = `v${pkg.version}`;

const TARGETS = {
  'darwin-x64':   'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'linux-x64':    'x86_64-unknown-linux-gnu',
  'linux-arm64':  'aarch64-unknown-linux-gnu',
};

const key = `${process.platform}-${process.arch}`;
const target = TARGETS[key];
if (!target) {
  console.error(`metaphor-cli: unsupported platform ${key}`);
  process.exit(1);
}

const asset = `metaphor-${target}.tar.gz`;
const url = `https://github.com/${REPO}/releases/download/${VERSION}/${asset}`;
const distDir = path.join(__dirname, 'dist');
const tarPath = path.join(distDir, asset);

fs.mkdirSync(distDir, { recursive: true });

function download(u, dest) {
  return new Promise((resolve, reject) => {
    const req = https.get(u, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return resolve(download(res.headers.location, dest));
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`download failed: ${res.statusCode} ${u}`));
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on('finish', () => file.close(() => resolve()));
      file.on('error', reject);
    });
    req.on('error', reject);
  });
}

(async () => {
  console.log(`Downloading metaphor ${VERSION} (${target})...`);
  await download(url, tarPath);
  execSync(`tar -xzf "${tarPath}" -C "${distDir}"`);
  fs.unlinkSync(tarPath);
  fs.chmodSync(path.join(distDir, 'metaphor'), 0o755);
  console.log('metaphor installed.');
})().catch((err) => {
  console.error(`metaphor-cli: install failed: ${err.message}`);
  process.exit(1);
});
