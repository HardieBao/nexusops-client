#!/usr/bin/env node
// Generates the website download manifest (manifest.json) from a directory of
// downloaded NexusOps Client release assets. This optional manifest is for a
// future NexusOps-owned download mirror; GitHub Releases remains authoritative.
//
// Usage: node scripts/generate-download-manifest.mjs <assets-dir> <tag> <base-url> <commit> [output] [pub-date]

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { getReleaseAssetContract } from "./release-asset-contract.mjs";

const [assetsDir, tag, baseUrl, commit, output = "manifest.json", pubDateArg] =
  process.argv.slice(2);

if (!assetsDir || !tag || !baseUrl || !commit) {
  console.error(
    "Usage: node scripts/generate-download-manifest.mjs <assets-dir> <tag> <base-url> <commit> [output] [pub-date]",
  );
  process.exit(1);
}

// Prefer the release's real publishedAt (passed by CI) over generation time.
const pubDate = pubDateArg ? new Date(pubDateArg) : new Date();
if (Number.isNaN(pubDate.getTime())) {
  console.error(`Invalid pub-date: ${pubDateArg}`);
  process.exit(1);
}

const normalizedBase = baseUrl.replace(/\/+$/, "");
if (new URL(`${normalizedBase}/`).protocol !== "https:") {
  console.error(`Download base URL must use HTTPS: ${baseUrl}`);
  process.exit(1);
}
if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  console.error(`Release tag is not a supported semantic version: ${tag}`);
  process.exit(1);
}
if (!/^[0-9a-f]{40}$/i.test(commit)) {
  console.error(
    `Release commit must be a full 40-character Git SHA: ${commit}`,
  );
  process.exit(1);
}

const contract = getReleaseAssetContract(tag);
const expected = contract.downloads;
const identities = new Set();
for (const entry of expected) {
  const identity = `${entry.platform}/${entry.arch}/${entry.kind}`;
  if (identities.has(identity)) {
    console.error(
      `Duplicate download platform entry in manifest rules: ${identity}`,
    );
    process.exit(1);
  }
  identities.add(identity);
}

const directoryNames = readdirSync(assetsDir).sort();
for (const name of directoryNames) {
  if (!contract.requiredReleaseNames.has(name)) {
    console.error(`Unexpected release asset: ${name}`);
    process.exit(1);
  }
}

const files = [];

for (const entry of expected) {
  const path = join(assetsDir, entry.name);
  if (!statExists(path)) {
    console.error(`Required download artifact is missing: ${path}`);
    process.exit(1);
  }
  files.push({
    platform: entry.platform,
    kind: entry.kind,
    arch: entry.arch,
    name: entry.name,
    size: statSync(path).size,
    sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
    url: `${normalizedBase}/${tag}/${encodeURIComponent(entry.name)}`,
  });
}

function statExists(path) {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

const manifest = {
  version: tag.replace(/^v/, ""),
  tag,
  commit: commit.toLowerCase(),
  pubDate: pubDate.toISOString(),
  files,
};

writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote ${output} with ${files.length} files for ${tag}`);
