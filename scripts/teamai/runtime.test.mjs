import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const root = fileURLToPath(new URL("../../", import.meta.url));
const bundle = path.join(root, ".teamai-build");
const executable = path.join(bundle, "node.exe");
const worker = path.join(bundle, "worker.mjs");
const spec = JSON.parse(
  fs.readFileSync(new URL("runtime.json", import.meta.url), "utf8"),
);
const digest = (file) =>
  createHash("sha256").update(fs.readFileSync(file)).digest("hex");
// Deliberately omit PATH, NODE_OPTIONS, HOME and all application credentials.
const environment = process.env.SystemRoot
  ? { SystemRoot: process.env.SystemRoot }
  : {};
function launch(args, input) {
  const result = spawnSync(executable, args, {
    env: environment,
    input: input === undefined ? undefined : JSON.stringify(input),
    encoding: "utf8",
    timeout: 10000,
    maxBuffer: 5 * 1024 * 1024,
    windowsHide: true,
    cwd: bundle,
  });
  assert.ifError(result.error);
  return result;
}
function call(operation, input) {
  const args = ["--permission", `--allow-fs-read=${bundle}`];
  if (input) args.push(`--allow-fs-read=${input.root}`);
  args.push(worker);
  const result = launch(args, {
    schema_version: 1,
    request_id: "runtime-test",
    operation,
    ...(input ? { input } : {}),
  });
  assert.equal(result.status, 0, result.stderr);
  const response = JSON.parse(result.stdout);
  assert.equal(response.request_id, "runtime-test");
  assert.equal(response.ok, true);
  return response.data;
}

test("bundled runtime and licenses match the pinned distribution and worker manifest", () => {
  assert.equal(
    process.platform,
    "win32",
    "Run this Windows acceptance command on Windows",
  );
  const manifest = JSON.parse(
    fs.readFileSync(path.join(bundle, "manifest.json"), "utf8"),
  );
  assert.equal(digest(executable), spec.executable.sha256);
  assert.equal(digest(path.join(bundle, "LICENSE-Node")), spec.license.sha256);
  assert.equal(manifest.runtime.sha256, spec.executable.sha256);
  assert.equal(manifest.worker_sha256, digest(worker));
  for (const dependency of manifest.dependencies)
    for (const file of dependency.files)
      assert.ok(fs.statSync(path.join(bundle, file)).size > 0);
  assert.equal(launch(["--version"]).stdout.trim(), `v${spec.version}`);
});

test("actual TeamAI worker runs without PATH or a global TeamAI installation", () => {
  assert.equal(
    call("version").upstream_commit,
    "6ae0619d067b1699bb2c6e435abf3ffe11a21d71",
  );
});

test("Chinese and spaced input paths work read-only for Skill and both Rule targets", (t) => {
  const fixture = fs.mkdtempSync(
    path.join(os.tmpdir(), "nexusops-teamai-runtime-"),
  );
  t.after(() => {
    const resolved = path.resolve(fixture);
    assert.equal(path.dirname(resolved), path.resolve(os.tmpdir()));
    assert.ok(path.basename(resolved).startsWith("nexusops-teamai-runtime-"));
    fs.rmSync(resolved, { recursive: true, force: true });
  });
  const inputRoot = path.join(fixture, "中文 资源");
  fs.mkdirSync(inputRoot);
  const skill = path.join(inputRoot, "SKILL.md");
  const rule = path.join(inputRoot, "review.md");
  fs.writeFileSync(
    skill,
    "---\nname: example\ndescription: 中文说明\n---\nExample\n",
  );
  fs.writeFileSync(rule, "---\nname: review\n---\n检查修改\n");
  const before = [digest(skill), digest(rule)];
  assert.equal(
    call("inspect_skill", { root: inputRoot, entry: "SKILL.md" }).name,
    "example",
  );
  for (const target of ["codex", "claude-code"])
    assert.equal(
      call("convert_rule", { root: inputRoot, entry: "review.md", target })
        .kind,
      "rule",
    );
  assert.deepEqual([digest(skill), digest(rule)], before);
});

test("runtime permissions refuse ungranted reads and writes", () => {
  const result = launch([
    "--permission",
    "--input-type=module",
    "-e",
    "import fs from 'node:fs'; for(const action of [()=>fs.readFileSync('manifest.json'),()=>fs.writeFileSync('must-not-create','x')]) {try {action();process.exit(2)} catch(e) {if(e.code!=='ERR_ACCESS_DENIED')process.exit(3)}}",
  ]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.existsSync(path.join(bundle, "must-not-create")), false);
});

test("runtime permissions refuse child process creation", () => {
  const result = launch([
    "--permission",
    "--input-type=module",
    "-e",
    "import {spawnSync} from 'node:child_process'; try {spawnSync(process.execPath,['--version']);process.exit(2)} catch(e) {if(e.code!=='ERR_ACCESS_DENIED')process.exit(3)}",
  ]);
  assert.equal(result.status, 0, result.stderr);
});
