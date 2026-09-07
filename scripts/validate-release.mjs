#!/usr/bin/env node

import { readFileSync } from 'node:fs';

const [tag, repository] = process.argv.slice(2);

function fail(message) {
  console.error(`Release validation failed: ${message}`);
  process.exit(1);
}

if (!tag || !repository) {
  fail('usage: node scripts/validate-release.mjs <vX.Y.Z tag> <owner/repository>');
}

if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  fail(`tag ${tag} is not a supported semantic version tag`);
}

if (!/^[^/]+\/[^/]+$/.test(repository)) {
  fail(`repository ${repository} must use owner/name form`);
}

const packageJson = JSON.parse(readFileSync('package.json', 'utf8'));
const tauriConfig = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
const cargoToml = readFileSync('src-tauri/Cargo.toml', 'utf8');
const packageStart = cargoToml.indexOf('[package]');
if (packageStart < 0) {
  fail('src-tauri/Cargo.toml has no [package] section');
}
const packageRemainder = cargoToml.slice(packageStart + '[package]'.length);
const nextSection = packageRemainder.search(/^\[/m);
const packageSection = nextSection < 0 ? packageRemainder : packageRemainder.slice(0, nextSection);
const cargoVersion = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const version = tag.slice(1);

for (const [source, actual] of [
  ['package.json', packageJson.version],
  ['src-tauri/tauri.conf.json', tauriConfig.version],
  ['src-tauri/Cargo.toml', cargoVersion],
]) {
  if (actual !== version) {
    fail(`${source} version ${actual ?? '<missing>'} does not match tag ${tag}`);
  }
}

if (packageJson.name !== 'nexusops-client') {
  fail(`package name must be nexusops-client, found ${packageJson.name}`);
}
if (tauriConfig.productName !== 'NexusOps Client') {
  fail(`Tauri productName must be NexusOps Client, found ${tauriConfig.productName}`);
}
if (tauriConfig.identifier !== 'io.nexusops.client') {
  fail(`Tauri identifier must be io.nexusops.client, found ${tauriConfig.identifier}`);
}
if (tauriConfig.bundle?.createUpdaterArtifacts !== true) {
  fail('Tauri updater artifacts must be enabled');
}

const endpoints = tauriConfig.plugins?.updater?.endpoints;
const expectedEndpoint = `https://github.com/${repository}/releases/latest/download/latest.json`;
if (!Array.isArray(endpoints) || endpoints.length !== 1 || endpoints[0] !== expectedEndpoint) {
  fail(`updater endpoint must be exactly ${expectedEndpoint}`);
}

const publicKey = tauriConfig.plugins?.updater?.pubkey;
if (typeof publicKey !== 'string' || publicKey.length < 40) {
  fail('Tauri updater public key is missing');
}
try {
  const decoded = Buffer.from(publicKey, 'base64').toString('utf8');
  if (!decoded.startsWith('untrusted comment:') || !decoded.includes('\nRW')) {
    fail('Tauri updater public key does not decode to a minisign public key');
  }
} catch (error) {
  fail(`Tauri updater public key is not valid base64: ${error.message}`);
}

console.log(`Release inputs are consistent for NexusOps Client ${tag} in ${repository}.`);
