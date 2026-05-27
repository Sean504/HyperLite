#!/usr/bin/env node
'use strict';

const { execFileSync, execSync } = require('child_process');
const path = require('path');
const os   = require('os');
const fs   = require('fs');

const PLATFORM_PACKAGES = {
  'win32-x64':    { pkg: '@hyperlite-ai/win32-x64',    bin: 'hl.exe' },
  'linux-x64':    { pkg: '@hyperlite-ai/linux-x64',    bin: 'hl'     },
  'linux-arm64':  { pkg: '@hyperlite-ai/linux-arm64',  bin: 'hl'     },
  'darwin-arm64': { pkg: '@hyperlite-ai/darwin-arm64', bin: 'hl'     },
  'darwin-x64':   { pkg: '@hyperlite-ai/darwin-x64',   bin: 'hl'     },
};

// WSL with Windows Node.js reports platform as 'win32' even though we're on Linux
function inWSL() {
  return os.platform() === 'win32' && !!process.env.WSL_DISTRO_NAME;
}

function getPlatformKey() {
  const plat = inWSL() ? 'linux' : os.platform();
  const raw  = os.arch();
  const arch = raw === 'x64' ? 'x64' : raw === 'arm64' ? 'arm64' : raw;
  return `${plat}-${arch}`;
}

function tryResolve(pkg) {
  try { return require.resolve(`${pkg}/package.json`); } catch { return null; }
}

// Convert a Windows absolute path to its /mnt/X/... WSL equivalent
function toWSLPath(winPath) {
  return winPath
    .replace(/^([A-Za-z]):\\/, (_, d) => `/mnt/${d.toLowerCase()}/`)
    .replace(/\\/g, '/');
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
    process.stderr.write(`hyperlite: installing platform package ${entry.pkg}...\n`);
    try {
      // --force is needed in WSL where Windows npm rejects linux packages
      const force = inWSL() ? '--force ' : '';
      execSync(`npm install -g ${force}${entry.pkg}`, { stdio: 'inherit' });
      pkgJson = tryResolve(entry.pkg);
    } catch { /* ignore — error shown below */ }
  }

  if (!pkgJson) {
    console.error(`hyperlite: could not install platform package "${entry.pkg}".`);
    console.error(`Run manually: npm install -g ${inWSL() ? '--force ' : ''}${entry.pkg}`);
    process.exit(1);
  }

  return path.join(path.dirname(pkgJson), entry.bin);
}

const bin = getBinaryPath();

try {
  if (inWSL()) {
    // Windows node can't exec Linux ELF binaries directly.
    // Use wsl.exe to run the binary in the WSL environment.
    const wslBin = toWSLPath(bin);
    execFileSync('wsl.exe', ['--', wslBin, ...process.argv.slice(2)], { stdio: 'inherit' });
  } else {
    execFileSync(bin, process.argv.slice(2), { stdio: 'inherit' });
  }
} catch (err) {
  process.exit(err.status ?? 1);
}
