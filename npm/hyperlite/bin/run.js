#!/usr/bin/env node
'use strict';

const { execFileSync, execSync } = require('child_process');
const path = require('path');
const os   = require('os');

const PLATFORM_PACKAGES = {
  'win32-x64':    { pkg: '@hyperlite-ai/win32-x64',    bin: 'hl.exe' },
  'linux-x64':    { pkg: '@hyperlite-ai/linux-x64',    bin: 'hl'     },
  'linux-arm64':  { pkg: '@hyperlite-ai/linux-arm64',  bin: 'hl'     },
  'darwin-arm64': { pkg: '@hyperlite-ai/darwin-arm64', bin: 'hl'     },
  'darwin-x64':   { pkg: '@hyperlite-ai/darwin-x64',   bin: 'hl'     },
};

function getPlatformKey() {
  const plat = os.platform();
  const raw  = os.arch();
  const arch = raw === 'x64' ? 'x64' : raw === 'arm64' ? 'arm64' : raw;
  return `${plat}-${arch}`;
}

function tryResolve(pkg) {
  try { return require.resolve(`${pkg}/package.json`); } catch { return null; }
}

function getBinaryPath() {
  const key   = getPlatformKey();
  const entry = PLATFORM_PACKAGES[key];

  if (!entry) {
    console.error(`hyperlite: unsupported platform "${key}"`);
    console.error(`Supported: ${Object.keys(PLATFORM_PACKAGES).join(', ')}`);
    process.exit(1);
  }

  let pkgJson = tryResolve(entry.pkg);

  if (!pkgJson) {
    // npm skipped the optional dep — install it now
    process.stderr.write(`hyperlite: installing ${entry.pkg}...\n`);
    try {
      execSync(`npm install -g ${entry.pkg}`, { stdio: 'inherit' });
      pkgJson = tryResolve(entry.pkg);
    } catch { /* reported below */ }
  }

  if (!pkgJson) {
    console.error(`hyperlite: could not install "${entry.pkg}".`);
    console.error(`Run manually: npm install -g ${entry.pkg}`);
    process.exit(1);
  }

  return path.join(path.dirname(pkgJson), entry.bin);
}

const bin = getBinaryPath();
try {
  execFileSync(bin, process.argv.slice(2), { stdio: 'inherit' });
} catch (err) {
  process.exit(err.status ?? 1);
}
