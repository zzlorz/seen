#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');

const PLATFORMS = {
  'darwin-arm64': { pkg: '@seen-tui/cli-darwin-arm64', bin: 'seen' },
  'darwin-x64':   { pkg: '@seen-tui/cli-darwin-x64',   bin: 'seen' },
  'linux-arm64':  { pkg: '@seen-tui/cli-linux-arm64',  bin: 'seen' },
  'linux-x64':    { pkg: '@seen-tui/cli-linux-x64',    bin: 'seen' },
  'win32-x64':    { pkg: '@seen-tui/cli-win32-x64',    bin: 'seen.exe' },
};

const key = `${process.platform}-${process.arch}`;
const entry = PLATFORMS[key];

if (!entry) {
  console.error(`seen: unsupported platform ${key}`);
  console.error(`Supported: ${Object.keys(PLATFORMS).join(', ')}`);
  process.exit(1);
}

let binary;
try {
  binary = require.resolve(`${entry.pkg}/bin/${entry.bin}`);
} catch (_) {
  console.error(`seen: platform package ${entry.pkg} is not installed.`);
  console.error(`This usually means your package manager skipped optionalDependencies.`);
  console.error(`Try reinstalling: npm install @seen-tui/cli`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error(result.error);
  process.exit(1);
}
process.exit(result.status == null ? 1 : result.status);