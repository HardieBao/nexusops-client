import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  existsSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  loadTauriUpdaterPublicKey,
  verifyTauriSignature,
} from "./generate-updater-manifest.mjs";
import { getReleaseAssetContract } from "./release-asset-contract.mjs";
import { syncVersionedAssets } from "./sync-r2-versioned.mjs";

const testApi = process.env.VITEST
  ? await import("vitest")
  : await import("node:test");
const test = testApi.test;
const after = testApi.after ?? testApi.afterAll;
const rootPath = process.cwd();
const temporaryRoot = mkdtempSync(join(tmpdir(), "nexusops-release-scripts-"));

after(() => rmSync(temporaryRoot, { recursive: true, force: true }));

function run(script, args, options = {}) {
  return execFileSync(process.execPath, [join(rootPath, script), ...args], {
    cwd: rootPath,
    encoding: "utf8",
    ...options,
  });
}

function createTauriSigningFixture() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const publicKeyDer = publicKey.export({ format: "der", type: "spki" });
  const rawPublicKey = publicKeyDer.subarray(-32);
  const keyId = Buffer.from("0102030405060708", "hex");
  const publicKeyPacket = Buffer.concat([
    Buffer.from("Ed"),
    keyId,
    rawPublicKey,
  ]);
  const publicKeyText = [
    "untrusted comment: minisign public key 0807060504030201",
    publicKeyPacket.toString("base64"),
    "",
  ].join("\n");

  return {
    encodedPublicKey: Buffer.from(publicKeyText).toString("base64"),
    signPayload(payload, fileName) {
      const trustedComment = `timestamp:1788710400\tfile:${fileName}`;
      const digest = createHash("blake2b512").update(payload).digest();
      const payloadSignature = sign(null, digest, privateKey);
      const signaturePacket = Buffer.concat([
        Buffer.from("ED"),
        keyId,
        payloadSignature,
      ]);
      const globalSignature = sign(
        null,
        Buffer.concat([payloadSignature, Buffer.from(trustedComment)]),
        privateKey,
      );
      const signatureText = [
        "untrusted comment: signature from minisign secret key",
        signaturePacket.toString("base64"),
        `trusted comment: ${trustedComment}`,
        globalSignature.toString("base64"),
        "",
      ].join("\n");
      return Buffer.from(signatureText).toString("base64");
    },
  };
}

function sha256(contents) {
  return createHash("sha256").update(contents).digest("hex");
}

function createFormalReleaseFixture(assets, tag, commit) {
  const contract = getReleaseAssetContract(tag);
  for (const name of contract.payloadNames) {
    writeFileSync(join(assets, name), `payload:${name}`);
  }

  for (const descriptor of contract.buildEvidence) {
    for (const name of descriptor.artifactNames) {
      if (!existsSync(join(assets, name))) {
        writeFileSync(join(assets, name), `metadata:${name}`);
      }
    }
    const artifacts = descriptor.artifactNames.map((path) => ({
      path,
      bytes: readFileSync(join(assets, path)).length,
      sha256: sha256(readFileSync(join(assets, path))),
    }));
    writeFileSync(
      join(assets, descriptor.evidenceName),
      JSON.stringify({
        schemaVersion: 1,
        product: "NexusOps Client",
        version: tag,
        commit,
        ref: tag,
        platform: descriptor.platform,
        architecture: descriptor.architecture,
        artifacts,
      }),
    );
    writeFileSync(
      join(assets, descriptor.sumsName),
      `${artifacts.map((artifact) => `${artifact.sha256}  ${artifact.path}`).join("\n")}\n`,
    );
  }

  writeFileSync(join(assets, "latest.json"), "{}");
  writeFileSync(join(assets, "NexusOps-Client-LICENSE.txt"), "MIT");
  writeFileSync(
    join(assets, "NexusOps-Client-UPSTREAM-NOTICE.md"),
    "upstream notice",
  );

  return {
    contract,
    artifactHashes: Object.fromEntries(
      [...contract.payloadNames].map((name) => [
        name,
        sha256(readFileSync(join(assets, name))),
      ]),
    ),
  };
}

function createFakeAws(initialObjects = new Map()) {
  const objects = new Map(initialObjects);
  const calls = [];
  const runAws = (args) => {
    calls.push(args);
    const operation = args[1];
    const bucket = args[args.indexOf("--bucket") + 1];
    assert.equal(bucket, "release-bucket");
    if (operation === "list-objects-v2") {
      const prefix = args[args.indexOf("--prefix") + 1];
      return JSON.stringify({
        IsTruncated: false,
        Contents: [...objects.keys()]
          .filter((key) => key.startsWith(prefix))
          .sort()
          .map((Key) => ({ Key })),
      });
    }
    const key = args[args.indexOf("--key") + 1];
    if (operation === "get-object") {
      const output = args[args.indexOf("--key") + 2];
      if (!objects.has(key)) throw new Error(`missing fake object ${key}`);
      writeFileSync(output, objects.get(key));
      return "{}";
    }
    if (operation === "put-object") {
      assert.equal(args[args.indexOf("--if-none-match") + 1], "*");
      if (objects.has(key))
        throw new Error(`conditional put failed for ${key}`);
      const body = args[args.indexOf("--body") + 1];
      objects.set(key, readFileSync(body));
      return "{}";
    }
    throw new Error(`unexpected fake aws operation ${operation}`);
  };
  return { objects, calls, runAws };
}

test("release identity matches the checked-in version and fork repository", () => {
  const packageJson = JSON.parse(
    readFileSync(join(rootPath, "package.json"), "utf8"),
  );
  const output = run("scripts/validate-release.mjs", [
    `v${packageJson.version}`,
    "HardieBao/nexusops-client",
  ]);
  assert.match(output, /Release inputs are consistent/);

  const mismatch = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/validate-release.mjs"),
      "v99.99.99",
      "HardieBao/nexusops-client",
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(mismatch.status, 0);
  assert.match(mismatch.stderr, /does not match tag/);
});

test("Tauri signature verification accepts an independent Minisign vector", () => {
  const publicKeyText = [
    "untrusted comment: minisign public key E7620F1842B4E81F",
    "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3",
    "",
  ].join("\n");
  const signatureText = [
    "untrusted comment: signature from minisign secret key",
    "RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=",
    "trusted comment: timestamp:1555779966\tfile:test",
    "QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==",
    "",
  ].join("\n");
  const configPath = join(temporaryRoot, "minisign-vector-tauri.conf.json");
  const payloadPath = join(temporaryRoot, "minisign-vector-payload");
  writeFileSync(
    configPath,
    JSON.stringify({
      plugins: {
        updater: { pubkey: Buffer.from(publicKeyText).toString("base64") },
      },
    }),
  );
  writeFileSync(payloadPath, "test");
  verifyTauriSignature(
    payloadPath,
    Buffer.from(signatureText).toString("base64"),
    loadTauriUpdaterPublicKey(configPath),
  );
});

test("updater manifest requires every artifact and signature", () => {
  const assets = join(temporaryRoot, "updater-assets");
  mkdirSync(assets);
  const signing = createTauriSigningFixture();
  const tauriConfig = join(temporaryRoot, "tauri.conf.json");
  writeFileSync(
    tauriConfig,
    JSON.stringify({
      plugins: { updater: { pubkey: signing.encodedPublicKey } },
    }),
  );
  const tag = "v0.1.0";
  const commit = "0123456789abcdef0123456789abcdef01234567";
  const names = [
    `NexusOps-Client-${tag}-macOS.tar.gz`,
    `NexusOps-Client-${tag}-Windows.msi`,
    `NexusOps-Client-${tag}-Windows-arm64.msi`,
    `NexusOps-Client-${tag}-Linux-x86_64.AppImage`,
    `NexusOps-Client-${tag}-Linux-arm64.AppImage`,
  ];
  for (const name of names) {
    const payload = Buffer.from(`artifact:${name}`);
    writeFileSync(join(assets, name), payload);
    writeFileSync(
      join(assets, `${name}.sig`),
      signing.signPayload(payload, name),
    );
  }

  const output = join(assets, "latest.json");
  run(
    "scripts/generate-updater-manifest.mjs",
    [assets, tag, "HardieBao/nexusops-client", commit, output, tauriConfig],
    { env: { ...process.env, SOURCE_DATE_EPOCH: "1788710400" } },
  );
  const manifest = JSON.parse(readFileSync(output, "utf8"));
  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.commit, commit);
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [
    "darwin-aarch64",
    "darwin-x86_64",
    "linux-aarch64",
    "linux-x86_64",
    "windows-aarch64",
    "windows-x86_64",
  ]);
  for (const platform of Object.values(manifest.platforms)) {
    assert.match(
      platform.url,
      /^https:\/\/github\.com\/HardieBao\/nexusops-client\/releases\/download\/v0\.1\.0\//,
    );
    assert.match(platform.signature, /^[A-Za-z0-9+/]+=*$/);
  }

  const tamperedArtifact = join(assets, names[4]);
  writeFileSync(tamperedArtifact, "tampered");
  const tampered = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/generate-updater-manifest.mjs"),
      assets,
      tag,
      "HardieBao/nexusops-client",
      commit,
      output,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(tampered.status, 0);
  assert.match(tampered.stderr, /signature verification failed/i);
  const restoredPayload = Buffer.from(`artifact:${names[4]}`);
  writeFileSync(tamperedArtifact, restoredPayload);

  const wrongSigningKey = createTauriSigningFixture();
  writeFileSync(
    `${tamperedArtifact}.sig`,
    wrongSigningKey.signPayload(restoredPayload, names[4]),
  );
  const wrongSignature = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/generate-updater-manifest.mjs"),
      assets,
      tag,
      "HardieBao/nexusops-client",
      commit,
      output,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(wrongSignature.status, 0);
  assert.match(
    wrongSignature.stderr,
    /wrong key|signature verification failed/i,
  );
  writeFileSync(
    `${tamperedArtifact}.sig`,
    signing.signPayload(restoredPayload, names[4]),
  );

  const mirroredOutput = join(temporaryRoot, "latest-r2.json");
  run("scripts/rewrite-updater-manifest.mjs", [
    output,
    tag,
    "https://downloads.nexusops.example/client",
    commit,
    mirroredOutput,
    tauriConfig,
  ]);
  const mirrored = JSON.parse(readFileSync(mirroredOutput, "utf8"));
  for (const platform of Object.values(mirrored.platforms)) {
    assert.match(
      platform.url,
      /^https:\/\/downloads\.nexusops\.example\/client\/v0\.1\.0\//,
    );
    assert.match(platform.signature, /^[A-Za-z0-9+/]+=*$/);
  }

  const conflictingManifestPath = join(assets, "latest-conflicting.json");
  const conflictingManifest = JSON.parse(readFileSync(output, "utf8"));
  conflictingManifest.platforms["darwin-x86_64"].signature =
    wrongSigningKey.signPayload(Buffer.from(`artifact:${names[0]}`), names[0]);
  writeFileSync(conflictingManifestPath, JSON.stringify(conflictingManifest));
  const conflictingSignature = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/rewrite-updater-manifest.mjs"),
      conflictingManifestPath,
      tag,
      "https://downloads.nexusops.example/client",
      commit,
      mirroredOutput,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(conflictingSignature.status, 0);
  assert.match(conflictingSignature.stderr, /conflicting signatures/i);

  const missingMirrorArtifact = join(assets, names[3]);
  rmSync(missingMirrorArtifact);
  const missingMirror = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/rewrite-updater-manifest.mjs"),
      output,
      tag,
      "https://downloads.nexusops.example/client",
      commit,
      mirroredOutput,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(missingMirror.status, 0);
  assert.match(missingMirror.stderr, /referenced updater artifact is missing/i);
  const mirrorPayload = Buffer.from(`artifact:${names[3]}`);
  writeFileSync(missingMirrorArtifact, mirrorPayload);

  writeFileSync(missingMirrorArtifact, "tampered mirror payload");
  const tamperedMirror = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/rewrite-updater-manifest.mjs"),
      output,
      tag,
      "https://downloads.nexusops.example/client",
      commit,
      mirroredOutput,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(tamperedMirror.status, 0);
  assert.match(tamperedMirror.stderr, /signature verification failed/i);
  writeFileSync(missingMirrorArtifact, mirrorPayload);

  rmSync(join(assets, `${names[4]}.sig`));
  const missing = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/generate-updater-manifest.mjs"),
      assets,
      tag,
      "HardieBao/nexusops-client",
      commit,
      output,
      tauriConfig,
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /Required updater signature is missing/);
});

test("download manifest requires the exact NexusOps release asset set", () => {
  const assets = join(temporaryRoot, "download-assets");
  mkdirSync(assets);
  const tag = "v0.1.0";
  const commit = "0123456789abcdef0123456789abcdef01234567";
  const prefix = `NexusOps-Client-${tag}`;
  const names = [
    `${prefix}-macOS.dmg`,
    `${prefix}-macOS.zip`,
    `${prefix}-Windows-arm64-Portable.zip`,
    `${prefix}-Windows-Portable.zip`,
    `${prefix}-Windows-arm64.msi`,
    `${prefix}-Windows.msi`,
    `${prefix}-Linux-arm64.AppImage`,
    `${prefix}-Linux-x86_64.AppImage`,
    `${prefix}-Linux-arm64.deb`,
    `${prefix}-Linux-x86_64.deb`,
    `${prefix}-Linux-arm64.rpm`,
    `${prefix}-Linux-x86_64.rpm`,
  ];
  for (const name of names)
    writeFileSync(join(assets, name), `artifact:${name}`);

  const output = join(temporaryRoot, "download-manifest.json");
  run("scripts/generate-download-manifest.mjs", [
    assets,
    tag,
    "https://downloads.nexusops.example/client",
    commit,
    output,
    "2026-09-07T00:00:00Z",
  ]);
  const manifest = JSON.parse(readFileSync(output, "utf8"));
  assert.equal(manifest.files.length, 12);
  assert.equal(manifest.commit, commit);
  assert.deepEqual(
    manifest.files.map((entry) => entry.name).sort(),
    names.sort(),
  );

  rmSync(join(assets, names[0]));
  const missing = spawnSync(
    process.execPath,
    [
      join(rootPath, "scripts/generate-download-manifest.mjs"),
      assets,
      tag,
      "https://downloads.nexusops.example/client",
      commit,
      output,
      "2026-09-07T00:00:00Z",
    ],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /Required download artifact is missing/);
  writeFileSync(join(assets, names[0]), `artifact:${names[0]}`);

  for (const extra of [
    `CC-Switch-${tag}-Windows.msi`,
    `NexusOps-Client-${tag}-debug.exe`,
    "unknown-metadata.json",
  ]) {
    writeFileSync(join(assets, extra), "unexpected release file");
    const unexpected = spawnSync(
      process.execPath,
      [
        join(rootPath, "scripts/generate-download-manifest.mjs"),
        assets,
        tag,
        "https://downloads.nexusops.example/client",
        commit,
        output,
        "2026-09-07T00:00:00Z",
      ],
      { cwd: rootPath, encoding: "utf8" },
    );
    assert.notEqual(unexpected.status, 0);
    assert.match(unexpected.stderr, /Unexpected release asset/);
    rmSync(join(assets, extra));
  }
  assert.equal(
    getReleaseAssetContract(tag).requiredReleaseNames.has(
      `NexusOps-Client-${tag}-Windows.MSI`,
    ),
    false,
  );
});

test("R2 prune selection always retains the currently published tag", () => {
  const script = join(rootPath, "scripts/rewrite-updater-manifest.mjs");
  const result = spawnSync(
    process.execPath,
    [script, "--prune-list", "v1.0.0", "5"],
    {
      cwd: rootPath,
      encoding: "utf8",
      input: [
        "v1.0.0",
        "v2.0.0",
        "v3.0.0",
        "v4.0.0",
        "v5.0.0",
        "v6.0.0",
        "v7.0.0",
      ].join("\n"),
    },
  );
  assert.equal(result.status, 0, result.stderr);
  const pruned = result.stdout.trim().split(/\r?\n/).filter(Boolean);
  assert.deepEqual(pruned, ["v2.0.0", "v3.0.0"]);
  assert.ok(!pruned.includes("v1.0.0"));

  const missingCurrent = spawnSync(
    process.execPath,
    [script, "--prune-list", "v1.0.0", "5"],
    {
      cwd: rootPath,
      encoding: "utf8",
      input: ["v2.0.0", "v3.0.0"].join("\n"),
    },
  );
  assert.notEqual(missingCurrent.status, 0);
  assert.match(missingCurrent.stderr, /current release tag is missing/i);
});

test("build evidence hashes every staged file and states its scope", () => {
  const assets = join(temporaryRoot, "build-assets");
  mkdirSync(assets);
  writeFileSync(join(assets, "nexusops-client-test"), "hello");
  const jsonOutput = join(assets, "evidence.json");
  const sumsOutput = join(assets, "SHA256SUMS.txt");
  run("scripts/generate-build-evidence.mjs", [assets, jsonOutput, sumsOutput], {
    env: {
      ...process.env,
      BUILD_COMMIT: "0123456789abcdef",
      BUILD_REF: "test",
      BUILD_PLATFORM: "test-os",
      BUILD_ARCH: "test-arch",
      BUILD_CARGO_VERSION: "cargo 1.95.0",
      BUILD_RUNNER_IMAGE_VERSION: "20260901.1",
    },
  });

  const evidence = JSON.parse(readFileSync(jsonOutput, "utf8"));
  assert.equal(evidence.artifacts.length, 1);
  assert.equal(
    evidence.artifacts[0].sha256,
    createHash("sha256").update("hello").digest("hex"),
  );
  assert.match(evidence.scope, /does not claim installation/);
  assert.equal(evidence.toolchain.cargo, "cargo 1.95.0");
  assert.equal(evidence.toolchain.runnerImageVersion, "20260901.1");
  assert.match(readFileSync(sumsOutput, "utf8"), /nexusops-client-test/);
});

test("G3 promotion evidence binds passed checks to the exact release payloads", () => {
  const assets = join(temporaryRoot, "g3-assets");
  mkdirSync(assets);
  const tag = "v0.1.0";
  const commit = "0123456789abcdef0123456789abcdef01234567";
  const { contract, artifactHashes } = createFormalReleaseFixture(
    assets,
    tag,
    commit,
  );
  const checks = Object.fromEntries(
    [
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
    ].map((name) => [name, "passed"]),
  );
  const evidencePath = join(assets, "G3-EVIDENCE.json");
  const evidence = {
    schemaVersion: 1,
    decision: "passed",
    tag,
    commit,
    reviewer: "release-reviewer",
    reviewedAt: "2026-09-07T00:00:00Z",
    evidenceLocation: "controlled://g3/v0.1.0",
    checks,
    artifacts: artifactHashes,
  };
  writeFileSync(evidencePath, JSON.stringify(evidence));

  const validator = join(rootPath, "scripts/generate-build-evidence.mjs");
  const valid = spawnSync(
    process.execPath,
    [validator, "--validate-g3", evidencePath, tag, commit, assets],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.equal(valid.status, 0, valid.stderr);
  assert.match(valid.stdout, /G3 promotion evidence verified/);

  const firstPayload = [...contract.payloadNames][0];
  const originalPayload = readFileSync(join(assets, firstPayload));
  writeFileSync(join(assets, firstPayload), "tampered installer");
  const tampered = spawnSync(
    process.execPath,
    [validator, "--validate-g3", evidencePath, tag, commit, assets],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(tampered.status, 0);
  assert.match(tampered.stderr, /does not match/);

  writeFileSync(join(assets, firstPayload), originalPayload);
  const incompleteEvidence = { ...evidence, checks: { ...checks } };
  delete incompleteEvidence.checks.invalidSignatureRejected;
  writeFileSync(evidencePath, JSON.stringify(incompleteEvidence));
  const incomplete = spawnSync(
    process.execPath,
    [validator, "--validate-g3", evidencePath, tag, commit, assets],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(incomplete.status, 0);
  assert.match(incomplete.stderr, /invalidSignatureRejected must be passed/);

  writeFileSync(evidencePath, JSON.stringify(evidence));
  const buildEvidencePath = join(
    assets,
    contract.buildEvidence[0].evidenceName,
  );
  const buildEvidence = JSON.parse(readFileSync(buildEvidencePath, "utf8"));
  buildEvidence.commit = "fedcba9876543210fedcba9876543210fedcba98";
  writeFileSync(buildEvidencePath, JSON.stringify(buildEvidence));
  const wrongBuildCommit = spawnSync(
    process.execPath,
    [validator, "--validate-g3", evidencePath, tag, commit, assets],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(wrongBuildCommit.status, 0);
  assert.match(wrongBuildCommit.stderr, /Build evidence .* does not match/);
  buildEvidence.commit = commit;
  writeFileSync(buildEvidencePath, JSON.stringify(buildEvidence));

  const debugPayload = `NexusOps-Client-${tag}-debug.exe`;
  writeFileSync(join(assets, debugPayload), "debug");
  const extraPayload = spawnSync(
    process.execPath,
    [validator, "--validate-g3", evidencePath, tag, commit, assets],
    { cwd: rootPath, encoding: "utf8" },
  );
  assert.notEqual(extraPayload.status, 0);
  assert.match(extraPayload.stderr, /Release asset set does not match/);
});

test("R2 versioned sync never replaces an object under the same tag", () => {
  const assets = join(temporaryRoot, "r2-versioned-assets");
  mkdirSync(assets);
  const tag = "v0.1.0";
  const commit = "0123456789abcdef0123456789abcdef01234567";
  const { contract } = createFormalReleaseFixture(assets, tag, commit);
  writeFileSync(join(assets, "G3-EVIDENCE.json"), "{}");
  const uploadNames = [...contract.requiredReleaseNames]
    .filter((name) => name !== "latest.json" && !name.endsWith(".sig"))
    .sort();
  const completeObjects = new Map(
    uploadNames.map((name) => [
      `${tag}/${name}`,
      readFileSync(join(assets, name)),
    ]),
  );

  const same = createFakeAws(completeObjects);
  assert.deepEqual(
    syncVersionedAssets({
      assetsDir: assets,
      bucket: "release-bucket",
      tag,
      endpoint: "https://fixture.r2.example",
      runAws: same.runAws,
    }),
    { existing: uploadNames.length, created: 0 },
  );
  assert.equal(same.calls.filter((args) => args[1] === "put-object").length, 0);

  const differentObjects = new Map(completeObjects);
  differentObjects.set(`${tag}/${uploadNames[0]}`, Buffer.from("different"));
  const different = createFakeAws(differentObjects);
  assert.throws(
    () =>
      syncVersionedAssets({
        assetsDir: assets,
        bucket: "release-bucket",
        tag,
        endpoint: "https://fixture.r2.example",
        runAws: different.runAws,
      }),
    /differs from the accepted release/,
  );
  assert.equal(
    different.calls.filter((args) => args[1] === "put-object").length,
    0,
  );

  const missingObjects = new Map(completeObjects);
  const missingName = uploadNames.at(-1);
  missingObjects.delete(`${tag}/${missingName}`);
  const missing = createFakeAws(missingObjects);
  const first = syncVersionedAssets({
    assetsDir: assets,
    bucket: "release-bucket",
    tag,
    endpoint: "https://fixture.r2.example",
    runAws: missing.runAws,
  });
  assert.deepEqual(first, { existing: uploadNames.length - 1, created: 1 });
  assert.equal(
    missing.calls.filter((args) => args[1] === "put-object").length,
    1,
  );
  const second = syncVersionedAssets({
    assetsDir: assets,
    bucket: "release-bucket",
    tag,
    endpoint: "https://fixture.r2.example",
    runAws: missing.runAws,
  });
  assert.deepEqual(second, { existing: uploadNames.length, created: 0 });
  assert.equal(
    missing.calls.filter((args) => args[1] === "put-object").length,
    1,
  );

  const racingObjects = new Map(completeObjects);
  racingObjects.delete(`${tag}/${missingName}`);
  const racing = createFakeAws(racingObjects);
  const runRacingAws = (args) => {
    if (args[1] === "put-object") {
      const key = args[args.indexOf("--key") + 1];
      racing.objects.set(key, Buffer.from("concurrent writer"));
    }
    return racing.runAws(args);
  };
  assert.throws(
    () =>
      syncVersionedAssets({
        assetsDir: assets,
        bucket: "release-bucket",
        tag,
        endpoint: "https://fixture.r2.example",
        runAws: runRacingAws,
      }),
    /conditional put failed/,
  );
});

test("promotion calls the reusable R2 sync and macOS assets stay universal", () => {
  const releaseWorkflow = readFileSync(
    join(rootPath, ".github/workflows/release.yml"),
    "utf8",
  );
  const r2Workflow = readFileSync(
    join(rootPath, ".github/workflows/sync-r2.yml"),
    "utf8",
  );
  assert.match(
    releaseWorkflow,
    /sync-r2-after-promotion:[\s\S]*needs: promote-release[\s\S]*uses: \.\/\.github\/workflows\/sync-r2\.yml/,
  );
  assert.match(r2Workflow, /workflow_call:/);

  const prepareMac = releaseWorkflow.slice(
    releaseWorkflow.indexOf("- name: Prepare macOS Assets"),
    releaseWorkflow.indexOf("- name: Notarize macOS DMG"),
  );
  const verifyMac = releaseWorkflow.slice(
    releaseWorkflow.indexOf(
      "- name: Verify macOS code signing and notarization",
    ),
    releaseWorkflow.indexOf("- name: Prepare Windows Assets"),
  );
  for (const section of [prepareMac, verifyMac]) {
    assert.match(section, /target\/universal-apple-darwin/);
    assert.doesNotMatch(section, /target\/(?:aarch64|x86_64)-apple-darwin/);
    assert.match(section, /lipo -verify_arch x86_64 arm64/);
  }
  assert.match(
    prepareMac,
    /tar -xzf[\s\S]*UPDATER_APP[\s\S]*lipo -verify_arch/,
  );
});

test("workflows install the pinned pnpm without Corepack key lookup", () => {
  for (const workflow of ["ci.yml", "build-evidence.yml", "release.yml"]) {
    const source = readFileSync(
      join(rootPath, ".github/workflows", workflow),
      "utf8",
    );
    assert.match(source, /uses: pnpm\/action-setup@v4/);
    assert.match(source, /version: ["']10\.12\.3["']/);
    assert.doesNotMatch(source, /corepack (?:enable|install)/);
  }
});
