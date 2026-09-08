#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

import { getReleaseAssetContract } from "./release-asset-contract.mjs";

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function defaultRunAws(args) {
  const result = spawnSync("aws", args, { encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(
      `aws ${args.slice(0, 2).join(" ")} failed: ${(result.stderr || result.stdout || "unknown error").trim()}`,
    );
  }
  return result.stdout;
}

function listRemote(runAws, bucket, prefix, endpoint) {
  const output = runAws([
    "s3api",
    "list-objects-v2",
    "--bucket",
    bucket,
    "--prefix",
    prefix,
    "--endpoint-url",
    endpoint,
    "--output",
    "json",
    "--no-cli-pager",
  ]);
  const response = JSON.parse(output || "{}");
  if (response.IsTruncated === true) {
    throw new Error(`R2 object listing for ${prefix} was truncated`);
  }
  return new Set(
    (response.Contents ?? []).map((entry) => {
      if (typeof entry.Key !== "string") {
        throw new Error("R2 object listing contains an invalid key");
      }
      return entry.Key;
    }),
  );
}

function downloadRemote(runAws, bucket, key, endpoint, output) {
  runAws([
    "s3api",
    "get-object",
    "--bucket",
    bucket,
    "--key",
    key,
    output,
    "--endpoint-url",
    endpoint,
    "--output",
    "json",
    "--no-cli-pager",
  ]);
}

function contentType(name) {
  if (name.endsWith(".json")) return "application/json";
  if (name.endsWith(".md")) return "text/markdown; charset=utf-8";
  if (name.endsWith(".txt")) return "text/plain; charset=utf-8";
  if (name.endsWith(".zip")) return "application/zip";
  if (name.endsWith(".dmg")) return "application/x-apple-diskimage";
  if (name.endsWith(".msi")) return "application/x-msi";
  if (name.endsWith(".deb")) return "application/vnd.debian.binary-package";
  if (name.endsWith(".rpm")) return "application/x-rpm";
  if (name.endsWith(".tar.gz")) return "application/gzip";
  return "application/octet-stream";
}

export function syncVersionedAssets({
  assetsDir,
  bucket,
  tag,
  endpoint,
  runAws = defaultRunAws,
}) {
  if (!assetsDir || !bucket || !tag || !endpoint) {
    throw new Error(
      "Usage: node scripts/sync-r2-versioned.mjs <assets-dir> <bucket> <tag> <endpoint>",
    );
  }
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
    throw new Error(`Invalid release tag: ${tag}`);
  }
  const endpointUrl = new URL(endpoint);
  if (
    endpointUrl.protocol !== "https:" ||
    endpointUrl.username ||
    endpointUrl.password
  ) {
    throw new Error("R2 endpoint must be an HTTPS URL without credentials");
  }

  const contract = getReleaseAssetContract(tag);
  const directoryNames = readdirSync(assetsDir, { withFileTypes: true })
    .map((entry) => {
      if (!entry.isFile())
        throw new Error(`Unexpected release directory: ${entry.name}`);
      return entry.name;
    })
    .sort();
  const requiredNames = [...contract.requiredReleaseNames].sort();
  if (JSON.stringify(directoryNames) !== JSON.stringify(requiredNames)) {
    throw new Error(
      "Local release directory does not match the release contract",
    );
  }

  const uploadNames = requiredNames.filter(
    (name) => name !== "latest.json" && !name.endsWith(".sig"),
  );
  const prefix = `${tag}/`;
  const expectedKeys = new Set(uploadNames.map((name) => `${prefix}${name}`));
  const remoteKeys = listRemote(runAws, bucket, prefix, endpoint);
  for (const key of remoteKeys) {
    if (!expectedKeys.has(key)) {
      throw new Error(
        `R2 tag ${tag} contains unexpected immutable object ${key}`,
      );
    }
  }

  const verificationRoot = mkdtempSync(join(tmpdir(), "nexusops-r2-verify-"));
  const missing = [];
  try {
    for (const name of uploadNames) {
      const key = `${prefix}${name}`;
      if (!remoteKeys.has(key)) {
        missing.push(name);
        continue;
      }
      const downloaded = join(verificationRoot, name);
      downloadRemote(runAws, bucket, key, endpoint, downloaded);
      if (sha256(downloaded) !== sha256(join(assetsDir, name))) {
        throw new Error(
          `R2 immutable object ${key} differs from the accepted release; refusing every upload`,
        );
      }
    }

    for (const name of missing) {
      const path = join(assetsDir, name);
      const key = `${prefix}${name}`;
      const digest = sha256(path);
      runAws([
        "s3api",
        "put-object",
        "--bucket",
        bucket,
        "--key",
        key,
        "--body",
        path,
        "--if-none-match",
        "*",
        "--cache-control",
        "public, max-age=31536000, immutable",
        "--content-type",
        contentType(name),
        "--metadata",
        `sha256=${digest}`,
        "--endpoint-url",
        endpoint,
        "--output",
        "json",
        "--no-cli-pager",
      ]);
      const downloaded = join(verificationRoot, `created-${basename(name)}`);
      downloadRemote(runAws, bucket, key, endpoint, downloaded);
      if (sha256(downloaded) !== digest) {
        throw new Error(`R2 object ${key} failed post-write verification`);
      }
    }

    const finalKeys = listRemote(runAws, bucket, prefix, endpoint);
    if (
      JSON.stringify([...finalKeys].sort()) !==
      JSON.stringify([...expectedKeys].sort())
    ) {
      throw new Error(`R2 immutable object set changed while syncing ${tag}`);
    }
  } finally {
    rmSync(verificationRoot, { recursive: true, force: true });
  }

  return {
    existing: uploadNames.length - missing.length,
    created: missing.length,
  };
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  try {
    const [assetsDir, bucket, tag, endpoint] = process.argv.slice(2);
    const result = syncVersionedAssets({ assetsDir, bucket, tag, endpoint });
    console.log(
      `R2 immutable sync verified ${result.existing} existing and created ${result.created} object(s) for ${tag}.`,
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
