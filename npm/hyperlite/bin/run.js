#!/usr/bin/env node
'use strict';

const { execFileSync } = require('child_process');
const path = require('path');
const os = require('os');

const PLATFORM_PACKAGES = {
  'win32-x64':    { pkg: '@hyperlite-ai/win32-x64',    bin: 'hl.exe' },
  'linux-x64':    { pkg: '@hyperlite-ai/linux-x64',    bin: 'hl'     },
  'darwin-arm64': { pkg: '@hyperlite-ai/darwin-arm64', bin: 'hl'     },
  'darwin-x64':   { pkg: '@hyperlite-ai/darwin-x64',   bin: 'hl'     },
};

function getPlatformKey() {
  const plat = os.platform();
  const raw  = os.arch();
  const arch = raw === 'x64' ? 'x64' : raw === 'arm64' ? 'arm64' : raw;
  return `${plat}-${arch}`;
}

function getBinaryPath() {
  const key = getPlatformKey();
  const entry = PLATFORM_PACKAGES[key];

  if (!entry) {
    console.error(`hyperlite: unsupported platform "${key}"`);
    console.error(`Supported: ${Object.keys(PLATFORM_PACKAGES).join(', ')}`);
    process.exit(1);
  }

  try {
    const pkgJson = require.resolve(`${entry.pkg}/package.json`);
    return path.join(path.dirname(pkgJson), entry.bin);
  } catch {
    console.error(`hyperlite: platform package "${entry.pkg}" is not installed.`);
    console.error('Try: npm install -g hyperlite');
    process.exit(1);
  }
}

const bin = getBinaryPath();
try {
  execFileSync(bin, process.argv.slice(2), { stdio: 'inherit' });
} catch (err) {
  // Exit with the child's exit code so shell scripts see it correctly
  process.exit(err.status ?? 1);
}
