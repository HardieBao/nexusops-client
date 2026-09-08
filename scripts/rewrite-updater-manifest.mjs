#!/usr/bin/env node
// Rewrites the Tauri updater manifest (latest.json) so its download URLs point
// at the R2 mirror instead of GitHub Releases. Minisign signatures cover file
// contents, not URLs, so they stay valid unchanged — the updater still verifies
// every downloaded artifact against the pubkey baked into the app.
//
// Usage: node scripts/rewrite-updater-manifest.mjs <latest-json> <tag> <base-url> <commit> [output] [tauri-config]

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  loadTauriUpdaterPublicKey,
  updaterDefinitions,
  verifyTauriSignature,
} from './generate-updater-manifest.mjs';

function parseVersionTag(tag) {
  const match = /^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(tag);
  if (!match) throw new Error(`Invalid release tag in R2 bucket: ${tag}`);
  return {
    tag,
    numbers: match.slice(1, 4).map(Number),
    prerelease: match[4]?.split('.') ?? [],
  };
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left.numbers[index] !== right.numbers[index]) {
      return left.numbers[index] - right.numbers[index];
    }
  }
  if (left.prerelease.length === 0 || right.prerelease.length === 0) {
    return left.prerelease.length === right.prerelease.length
      ? 0
      : left.prerelease.length === 0
        ? 1
        : -1;
  }
  const length = Math.max(left.prerelease.length, right.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = left.prerelease[index];
    const rightPart = right.prerelease[index];
    if (leftPart === undefined || rightPart === undefined) {
      return leftPart === rightPart ? 0 : leftPart === undefined ? -1 : 1;
    }
    if (leftPart === rightPart) continue;
    const leftNumber = /^\d+$/.test(leftPart) ? Number(leftPart) : null;
    const rightNumber = /^\d+$/.test(rightPart) ? Number(rightPart) : null;
    if (leftNumber !== null && rightNumber !== null)
      return leftNumber - rightNumber;
    if (leftNumber !== null || rightNumber !== null)
      return leftNumber !== null ? -1 : 1;
    return leftPart.localeCompare(rightPart);
  }
  return 0;
}

export function selectVersionsToPrune(versionTags, currentTag, keepCount) {
  if (!Number.isInteger(keepCount) || keepCount < 1) {
    throw new Error(
      `KEEP_VERSIONS must be a positive integer, found ${keepCount}`,
    );
  }
  const versions = [...new Set(versionTags.filter(Boolean))].map(
    parseVersionTag,
  );
  if (!versions.some((version) => version.tag === currentTag)) {
    throw new Error(
      `Current release tag is missing from the R2 bucket: ${currentTag}`,
    );
  }
  const retained = new Set([currentTag]);
  const newestOthers = versions
    .filter((version) => version.tag !== currentTag)
    .sort((left, right) => compareVersions(right, left))
    .slice(0, keepCount - 1);
  for (const version of newestOthers) retained.add(version.tag);
  return versions
    .filter((version) => !retained.has(version.tag))
    .sort(compareVersions)
    .map((version) => version.tag);
}

function printPruneList() {
  const [, currentTag, keepValue] = process.argv.slice(2);
  const keepCount = Number(keepValue);
  const versions = readFileSync(0, 'utf8').split(/\r?\n/).filter(Boolean);
  const selected = selectVersionsToPrune(versions, currentTag, keepCount);
  if (selected.length > 0) process.stdout.write(`${selected.join('\n')}\n`);
}

function rewriteUpdaterManifest() {
  const [
    input,
    tag,
    baseUrl,
    commit,
    output = 'latest-r2.json',
    configPath = 'src-tauri/tauri.conf.json',
  ] = process.argv.slice(2);

  if (!input || !tag || !baseUrl || !commit) {
    throw new Error(
      'Usage: node scripts/rewrite-updater-manifest.mjs <latest-json> <tag> <base-url> <commit> [output] [tauri-config]',
    );
  }
  if (!/^[0-9a-f]{40}$/i.test(commit)) {
    throw new Error(
      `Release commit must be a full 40-character Git SHA: ${commit}`,
    );
  }

  const manifest = JSON.parse(readFileSync(input, 'utf8'));
  if (manifest.version !== tag.replace(/^v/, '')) {
    throw new Error(
      `Updater manifest version ${manifest.version ?? '<missing>'} does not match ${tag}`,
    );
  }
  if (manifest.commit !== commit.toLowerCase()) {
    throw new Error(
      `Updater manifest commit ${manifest.commit ?? '<missing>'} does not match ${commit}`,
    );
  }
  const platforms = manifest.platforms ?? {};
  const expectedEntries = updaterDefinitions(tag).flatMap((definition) =>
    definition.keys.map((key) => [key, definition.file]),
  );
  const actualKeys = Object.keys(platforms).sort();
  const expectedKeys = expectedEntries.map(([key]) => key).sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    throw new Error(
      `Updater manifest platforms must be exactly ${expectedKeys.join(', ')}, found ${actualKeys.join(', ') || '<none>'}`,
    );
  }

  const normalizedBase = baseUrl.replace(/\/+$/, '');
  const mirrorUrl = new URL(`${normalizedBase}/`);
  if (mirrorUrl.protocol !== 'https:') {
    throw new Error(`R2 public base URL must use HTTPS: ${baseUrl}`);
  }
  const publicKey = loadTauriUpdaterPublicKey(configPath);
  const verifiedArtifacts = new Map();

  for (const [key, expectedFile] of expectedEntries) {
    const entry = platforms[key];
    if (
      typeof entry?.url !== 'string' ||
      typeof entry?.signature !== 'string'
    ) {
      throw new Error(`Platform ${key} is missing url or signature`);
    }
    if (
      !entry.url.startsWith('https://github.com/') ||
      !entry.url.includes(`/releases/download/${tag}/`)
    ) {
      throw new Error(
        `Platform ${key} has unexpected url for ${tag}: ${entry.url}`,
      );
    }
    const encodedName = new URL(entry.url).pathname.split('/').pop();
    const name = encodedName ? decodeURIComponent(encodedName) : '';
    if (!name || basename(name) !== name || name !== expectedFile) {
      throw new Error(
        `Platform ${key} must reference ${expectedFile}, found ${name || '<missing>'}`,
      );
    }
    const artifactPath = join(dirname(input), name);
    if (!existsSync(artifactPath)) {
      throw new Error(
        `Referenced updater artifact is missing: ${artifactPath}`,
      );
    }
    const verifiedSignature = verifiedArtifacts.get(name);
    if (verifiedSignature && verifiedSignature !== entry.signature) {
      throw new Error(`Updater artifact ${name} has conflicting signatures`);
    }
    if (!verifiedSignature) {
      verifyTauriSignature(artifactPath, entry.signature, publicKey);
      verifiedArtifacts.set(name, entry.signature);
    }
    entry.url = `${normalizedBase}/${tag}/${encodeURIComponent(name)}`;
  }

  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Wrote ${output} with ${expectedEntries.length} verified platforms pointing at ${normalizedBase}/${tag}/`,
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    if (process.argv[2] === '--prune-list') {
      printPruneList();
    } else {
      rewriteUpdaterManifest();
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
