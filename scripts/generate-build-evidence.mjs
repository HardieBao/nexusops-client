#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";

import { getReleaseAssetContract } from "./release-asset-contract.mjs";

const REQUIRED_G3_CHECKS = [
  "windowsInstall",
  "macosInstall",
  "linuxInstall",
  "protocolImport",
  "teamConnectSyncDisconnect",
  "uninstallPreservesUserFiles",
  "windowsAuthenticode",
  "macosSigningNotarization",
  "windowsPathAndFileLock",
  "macosPermissions",
  "linuxLinksAndExecutableBits",
  "validUpdate",
  "invalidSignatureRejected",
  "tamperedPayloadRejected",
  "badUrlRecovery",
  "interruptedDownloadRecovery",
  "updaterUnavailableRecovery",
  "teamStatePreserved",
];

function listFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? listFiles(path) : [path];
  });
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function validateG3Evidence(evidencePath, tag, commit, assetsDir) {
  if (!evidencePath || !tag || !commit || !assetsDir) {
    throw new Error(
      "Usage: node scripts/generate-build-evidence.mjs --validate-g3 <evidence-json> <tag> <commit> <assets-dir>",
    );
  }
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
    throw new Error(`Invalid G3 evidence tag: ${tag}`);
  }
  if (!/^[0-9a-f]{40}$/i.test(commit)) {
    throw new Error(
      `G3 evidence commit must be a full 40-character Git SHA: ${commit}`,
    );
  }

  const assetsRoot = resolve(assetsDir);
  const contract = getReleaseAssetContract(tag);
  const directoryNames = readdirSync(assetsRoot, { withFileTypes: true })
    .map((entry) => {
      if (!entry.isFile()) {
        throw new Error(`Unexpected release asset directory: ${entry.name}`);
      }
      return entry.name;
    })
    .sort();
  const requiredNames = [...contract.requiredReleaseNames].sort();
  if (JSON.stringify(directoryNames) !== JSON.stringify(requiredNames)) {
    throw new Error(
      `Release asset set does not match the contract; expected ${requiredNames.join(", ")}, found ${directoryNames.join(", ") || "<none>"}`,
    );
  }

  validateBuildEvidence(contract, tag, commit, assetsRoot);

  const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  if (evidence.schemaVersion !== 1 || evidence.decision !== "passed") {
    throw new Error(
      'G3 evidence must use schemaVersion 1 with decision "passed"',
    );
  }
  if (evidence.tag !== tag || evidence.commit !== commit) {
    throw new Error(
      `G3 evidence identity ${evidence.tag ?? "<missing>"}/${evidence.commit ?? "<missing>"} does not match ${tag}/${commit}`,
    );
  }
  if (
    typeof evidence.reviewer !== "string" ||
    evidence.reviewer.trim().length === 0
  ) {
    throw new Error("G3 evidence reviewer is required");
  }
  if (
    typeof evidence.reviewedAt !== "string" ||
    Number.isNaN(new Date(evidence.reviewedAt).getTime())
  ) {
    throw new Error("G3 evidence reviewedAt must be a valid date");
  }
  if (
    typeof evidence.evidenceLocation !== "string" ||
    evidence.evidenceLocation.trim().length === 0
  ) {
    throw new Error("G3 evidenceLocation is required");
  }
  for (const check of REQUIRED_G3_CHECKS) {
    if (evidence.checks?.[check] !== "passed") {
      throw new Error(`G3 check ${check} must be passed`);
    }
  }

  const payloadPaths = [...contract.payloadNames]
    .sort()
    .map((name) => join(assetsRoot, name));
  const payloadNames = payloadPaths.map((path) => basename(path)).sort();
  const recordedNames = Object.keys(evidence.artifacts ?? {}).sort();
  if (JSON.stringify(payloadNames) !== JSON.stringify(recordedNames)) {
    throw new Error(
      `G3 artifact list does not match release payloads; expected ${payloadNames.join(", ")}, found ${recordedNames.join(", ") || "<none>"}`,
    );
  }
  for (const path of payloadPaths) {
    const name = basename(path);
    const recordedHash = evidence.artifacts[name];
    if (
      !/^[0-9a-f]{64}$/i.test(recordedHash) ||
      sha256(path) !== recordedHash.toLowerCase()
    ) {
      throw new Error(`Release payload ${name} does not match G3 evidence`);
    }
  }
  console.log(`G3 promotion evidence verified for ${tag} at ${commit}.`);
}

function validateBuildEvidence(contract, tag, commit, assetsRoot) {
  for (const descriptor of contract.buildEvidence) {
    const evidencePath = join(assetsRoot, descriptor.evidenceName);
    const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
    if (
      evidence.schemaVersion !== 1 ||
      evidence.product !== "NexusOps Client" ||
      evidence.version !== tag ||
      evidence.commit !== commit ||
      evidence.ref !== tag ||
      evidence.platform !== descriptor.platform ||
      evidence.architecture !== descriptor.architecture
    ) {
      throw new Error(
        `Build evidence ${descriptor.evidenceName} does not match ${tag}/${commit}/${descriptor.platform}/${descriptor.architecture}`,
      );
    }

    const artifactNames = (evidence.artifacts ?? []).map((entry) => entry.path);
    if (
      JSON.stringify(artifactNames) !== JSON.stringify(descriptor.artifactNames)
    ) {
      throw new Error(
        `Build evidence ${descriptor.evidenceName} has an unexpected artifact set`,
      );
    }
    for (const artifact of evidence.artifacts) {
      if (
        typeof artifact.path !== "string" ||
        basename(artifact.path) !== artifact.path ||
        !/^[0-9a-f]{64}$/i.test(artifact.sha256) ||
        sha256(join(assetsRoot, artifact.path)) !==
          artifact.sha256.toLowerCase()
      ) {
        throw new Error(
          `Build evidence ${descriptor.evidenceName} does not match ${artifact.path ?? "<missing>"}`,
        );
      }
    }

    const expectedSums = `${evidence.artifacts
      .map((artifact) => `${artifact.sha256.toLowerCase()}  ${artifact.path}`)
      .join("\n")}\n`;
    if (
      readFileSync(join(assetsRoot, descriptor.sumsName), "utf8") !==
      expectedSums
    ) {
      throw new Error(
        `Build checksums ${descriptor.sumsName} do not match ${descriptor.evidenceName}`,
      );
    }
  }
}

function generateBuildEvidence(assetsDir, jsonOutput, sumsOutput) {
  if (!assetsDir || !jsonOutput || !sumsOutput) {
    throw new Error(
      "Usage: node scripts/generate-build-evidence.mjs <assets-dir> <evidence.json> <SHA256SUMS.txt>",
    );
  }

  const assetsRoot = resolve(assetsDir);
  const excluded = new Set([resolve(jsonOutput), resolve(sumsOutput)]);
  const artifacts = listFiles(assetsRoot)
    .filter((path) => !excluded.has(resolve(path)))
    .map((path) => ({
      path: relative(assetsRoot, path).replaceAll("\\", "/"),
      bytes: statSync(path).size,
      sha256: sha256(path),
    }))
    .sort((left, right) => left.path.localeCompare(right.path));

  if (artifacts.length === 0) {
    throw new Error(`No files found in ${assetsRoot}`);
  }

  const required = [
    "BUILD_COMMIT",
    "BUILD_REF",
    "BUILD_PLATFORM",
    "BUILD_ARCH",
  ];
  for (const name of required) {
    if (!process.env[name]) throw new Error(`${name} is required`);
  }

  const evidence = {
    schemaVersion: 1,
    product: "NexusOps Client",
    version: process.env.BUILD_VERSION ?? null,
    commit: process.env.BUILD_COMMIT,
    ref: process.env.BUILD_REF,
    platform: process.env.BUILD_PLATFORM,
    architecture: process.env.BUILD_ARCH,
    sourceDate: process.env.BUILD_SOURCE_DATE ?? null,
    workflow: {
      repository: process.env.GITHUB_REPOSITORY ?? null,
      runId: process.env.GITHUB_RUN_ID ?? null,
      runAttempt: process.env.GITHUB_RUN_ATTEMPT ?? null,
    },
    toolchain: {
      runnerImage: process.env.BUILD_RUNNER_IMAGE ?? null,
      runnerImageVersion: process.env.BUILD_RUNNER_IMAGE_VERSION ?? null,
      node: process.version,
      pnpm: process.env.BUILD_PNPM_VERSION ?? null,
      rustc: process.env.BUILD_RUSTC_VERSION ?? null,
      cargo: process.env.BUILD_CARGO_VERSION ?? null,
    },
    scope:
      "Build and headless checks only; this record does not claim installation, OS code signing, notarization, or GUI execution.",
    artifacts,
  };

  mkdirSync(dirname(resolve(jsonOutput)), { recursive: true });
  mkdirSync(dirname(resolve(sumsOutput)), { recursive: true });
  writeFileSync(jsonOutput, `${JSON.stringify(evidence, null, 2)}\n`);
  writeFileSync(
    sumsOutput,
    `${artifacts.map((artifact) => `${artifact.sha256}  ${artifact.path}`).join("\n")}\n`,
  );
  console.log(
    `Recorded ${artifacts.length} artifact(s) in ${jsonOutput} and ${sumsOutput}.`,
  );
}

try {
  if (process.argv[2] === "--validate-g3") {
    validateG3Evidence(...process.argv.slice(3));
  } else {
    generateBuildEvidence(...process.argv.slice(2));
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
