#!/usr/bin/env node
// Shim that forwards invocation to the downloaded native binary.

const path = require('path');
const { spawnSync } = require('child_process');

const bin = path.join(__dirname, '..', 'dist', 'metaphor');
const result = spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  console.error(`metaphor: failed to launch binary: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
