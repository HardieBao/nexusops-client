#!/usr/bin/env node

import { createHash, createPublicKey, verify } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');

function decodeBase64(value, label) {
  const normalized = value.trim();
  if (
    normalized.length === 0 ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(normalized) ||
    normalized.length % 4 === 1
  ) {
    throw new Error(`${label} is not valid base64`);
  }
  const decoded = Buffer.from(normalized, 'base64');
  if (
    decoded.toString('base64').replace(/=+$/, '') !==
    normalized.replace(/=+$/, '')
  ) {
    throw new Error(`${label} is not canonical base64`);
  }
  return decoded;
}

export function updaterDefinitions(tag) {
  const prefix = `NexusOps-Client-${tag}`;
  return [
    {
      keys: ['darwin-aarch64', 'darwin-x86_64'],
      file: `${prefix}-macOS.tar.gz`,
    },
    { keys: ['windows-x86_64'], file: `${prefix}-Windows.msi` },
    { keys: ['windows-aarch64'], file: `${prefix}-Windows-arm64.msi` },
    { keys: ['linux-x86_64'], file: `${prefix}-Linux-x86_64.AppImage` },
    { keys: ['linux-aarch64'], file: `${prefix}-Linux-arm64.AppImage` },
  ];
}

export function loadTauriUpdaterPublicKey(
  configPath = 'src-tauri/tauri.conf.json',
) {
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  const encodedPublicKey = config.plugins?.updater?.pubkey;
  if (typeof encodedPublicKey !== 'string') {
    throw new Error(`Tauri updater public key is missing from ${configPath}`);
  }
  const publicKeyText = decodeBase64(
    encodedPublicKey,
    'Tauri updater public key',
  ).toString('utf8');
  const publicKeyLines = publicKeyText.split(/\r?\n/);
  if (
    !publicKeyLines[0]?.startsWith('untrusted comment:') ||
    !publicKeyLines[1]
  ) {
    throw new Error(
      `Tauri updater public key in ${configPath} is not a minisign public key`,
    );
  }
  const packet = decodeBase64(publicKeyLines[1], 'Minisign public key packet');
  if (
    packet.length !== 42 ||
    packet[0] !== 0x45 ||
    ![0x44, 0x64].includes(packet[1])
  ) {
    throw new Error(
      `Tauri updater public key in ${configPath} has an invalid packet`,
    );
  }
  return {
    keyId: packet.subarray(2, 10),
    key: createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, packet.subarray(10)]),
      format: 'der',
      type: 'spki',
    }),
  };
}

export function verifyTauriSignature(
  artifactPath,
  encodedSignature,
  publicKey,
) {
  const signatureText = decodeBase64(
    encodedSignature,
    `Updater signature for ${basename(artifactPath)}`,
  ).toString('utf8');
  const lines = signatureText.split(/\r?\n/);
  if (lines.length < 4 || !lines[2].startsWith('trusted comment: ')) {
    throw new Error(
      `Updater signature for ${basename(artifactPath)} has invalid minisign text`,
    );
  }
  const packet = decodeBase64(lines[1], 'Minisign signature packet');
  const globalSignature = decodeBase64(lines[3], 'Minisign global signature');
  if (
    packet.length !== 74 ||
    globalSignature.length !== 64 ||
    packet[0] !== 0x45 ||
    ![0x44, 0x64].includes(packet[1])
  ) {
    throw new Error(
      `Updater signature for ${basename(artifactPath)} has invalid packet data`,
    );
  }
  if (!packet.subarray(2, 10).equals(publicKey.keyId)) {
    throw new Error(
      `Updater signature for ${basename(artifactPath)} uses the wrong key`,
    );
  }

  const payload = readFileSync(artifactPath);
  const signedPayload =
    packet[1] === 0x44
      ? createHash('blake2b512').update(payload).digest()
      : payload;
  const payloadSignature = packet.subarray(10);
  if (!verify(null, signedPayload, publicKey.key, payloadSignature)) {
    throw new Error(
      `Updater signature verification failed for ${basename(artifactPath)}`,
    );
  }
  const trustedComment = lines[2].slice('trusted comment: '.length);
  if (
    !verify(
      null,
      Buffer.concat([payloadSignature, Buffer.from(trustedComment)]),
      publicKey.key,
      globalSignature,
    )
  ) {
    throw new Error(
      `Updater global signature verification failed for ${basename(artifactPath)}`,
    );
  }
}

function generateUpdaterManifest() {
  const [
    assetsDir,
    tag,
    repository,
    commit,
    output = 'latest.json',
    configPath = 'src-tauri/tauri.conf.json',
  ] = process.argv.slice(2);

  if (!assetsDir || !tag || !repository || !commit) {
    throw new Error(
      'Usage: node scripts/generate-updater-manifest.mjs <assets-dir> <tag> <owner/repository> <commit> [output] [tauri-config]',
    );
  }
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
    throw new Error(`Release tag is not a supported semantic version: ${tag}`);
  }
  if (!/^[^/]+\/[^/]+$/.test(repository)) {
    throw new Error(`Repository must use owner/name form: ${repository}`);
  }
  if (!/^[0-9a-f]{40}$/i.test(commit)) {
    throw new Error(
      `Release commit must be a full 40-character Git SHA: ${commit}`,
    );
  }

  const publicKey = loadTauriUpdaterPublicKey(configPath);
  const platforms = {};
  for (const definition of updaterDefinitions(tag)) {
    const artifactPath = join(assetsDir, definition.file);
    const signaturePath = `${artifactPath}.sig`;
    if (!existsSync(artifactPath)) {
      throw new Error(`Required updater artifact is missing: ${artifactPath}`);
    }
    if (!existsSync(signaturePath)) {
      throw new Error(
        `Required updater signature is missing: ${signaturePath}`,
      );
    }
    const signature = readFileSync(signaturePath, 'utf8').trim();
    if (!signature) {
      throw new Error(`Updater signature is empty: ${signaturePath}`);
    }
    verifyTauriSignature(artifactPath, signature, publicKey);
    const url = `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(
      basename(artifactPath),
    )}`;
    for (const key of definition.keys) {
      platforms[key] = { signature, url };
    }
  }

  const sourceEpoch = process.env.SOURCE_DATE_EPOCH;
  const publishedAt = sourceEpoch
    ? new Date(Number(sourceEpoch) * 1000).toISOString()
    : new Date().toISOString();
  const manifest = {
    version: tag.replace(/^v/, ''),
    commit: commit.toLowerCase(),
    notes: `NexusOps Client ${tag} (${commit})`,
    pub_date: publishedAt,
    platforms,
  };

  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Wrote ${output} with ${Object.keys(platforms).length} verified platform entries.`,
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    generateUpdaterManifest();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
