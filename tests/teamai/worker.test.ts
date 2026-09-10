import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

let fixture: string;
const worker = path.resolve(".teamai-build/worker.mjs");
beforeAll(() => {
  execFileSync(process.execPath, ["scripts/teamai/build.mjs"], {
    stdio: "pipe",
  });
});
beforeEach(() => {
  fixture = fs.mkdtempSync(path.join(os.tmpdir(), "nexusops-teamai-worker-"));
});
afterEach(() => {
  const resolved = path.resolve(fixture),
    temp = path.resolve(os.tmpdir());
  if (
    !resolved.startsWith(temp + path.sep) ||
    !path.basename(resolved).startsWith("nexusops-teamai-worker-")
  )
    throw new Error("Unsafe test cleanup target");
  fs.rmSync(resolved, { recursive: true, force: true });
});
function call(
  operation: string,
  input?: Record<string, unknown>,
  extra?: Record<string, unknown>,
) {
  const result = spawnSync(process.execPath, [worker], {
    input: JSON.stringify({
      schema_version: 1,
      request_id: "fixture-request",
      operation,
      ...(input ? { input } : {}),
      ...extra,
    }),
    encoding: "utf8",
    timeout: 15000,
    maxBuffer: 5 * 1024 * 1024,
  });
  expect(result.error).toBeUndefined();
  const response = JSON.parse(result.stdout);
  expect(response.request_id).toBe("fixture-request");
  return { response, result };
}
function skill(content: string) {
  const root = path.join(fixture, "中文 skill");
  fs.mkdirSync(root);
  fs.writeFileSync(path.join(root, "SKILL.md"), content);
  return root;
}
describe("pinned TeamAI worker", () => {
  it("reports the selected upstream revision from the real compiled worker", () => {
    const { response, result } = call("version");
    expect(result.status).toBe(0);
    expect(response.data.upstream_commit).toBe(
      "6ae0619d067b1699bb2c6e435abf3ffe11a21d71",
    );
    expect(response.data.operations).toEqual([
      "inspect_skill",
      "convert_rule",
      "version",
    ]);
  });
  it("uses upstream frontmatter parsing and returns bounded file observations without changing files", () => {
    const content =
      "\uFEFF---\r\nname: team-sample\r\ndescription: 中文说明\r\n---\r\n# Skill\r\n";
    const root = skill(content);
    fs.writeFileSync(path.join(root, "example.bin"), Buffer.from([1, 2, 3]));
    const { response, result } = call("inspect_skill", {
      root,
      entry: "SKILL.md",
    });
    expect(result.status).toBe(0);
    expect(response.ok).toBe(true);
    expect(response.data.name).toBe("team-sample");
    expect(response.data.description).toBe("中文说明");
    expect(response.data.files).toHaveLength(2);
    expect(response.data.files[0].sha256).toBe(
      createHash("sha256").update(Buffer.from(content)).digest("hex"),
    );
    expect(response.data.content).toBeUndefined();
    expect(fs.readFileSync(path.join(root, "SKILL.md"), "utf8")).toBe(content);
    expect(fs.readdirSync(root).sort()).toEqual(["SKILL.md", "example.bin"]);
  });
  it.each(["codex", "claude-code"])(
    "converts neutral rules for %s using upstream naming while preserving scope",
    (target) => {
      fs.writeFileSync(
        path.join(fixture, "rule.md"),
        "---\r\npaths: ['src/**']\r\n---\r\nReview carefully.\r\n",
      );
      const { response } = call("convert_rule", {
        root: fixture,
        entry: "rule.md",
        target,
      });
      expect(response.ok).toBe(true);
      expect(response.data.extension).toBe(".md");
      expect(response.data.name).toBe("rule");
      expect(response.data.content).toContain("paths: ['src/**']");
      expect(response.data.content).not.toContain("\r\n");
    },
  );
  it("rejects traversal and credential-bearing fields without echoing them", () => {
    const { response } = call("convert_rule", {
      root: fixture,
      entry: "../secret.md",
      target: "codex",
    });
    expect(response.error.code).toBe("unsafe_path");
    const result = spawnSync(process.execPath, [worker], {
      input: JSON.stringify({
        schema_version: 1,
        request_id: "fixture-request",
        operation: "version",
        member_key: "PRIVATE_SENTINEL",
      }),
      encoding: "utf8",
    });
    expect(result.stdout).not.toContain("PRIVATE_SENTINEL");
    expect(JSON.parse(result.stdout).ok).toBe(false);
  });
  it("rejects a linked directory instead of reading outside the chosen root", () => {
    const root = skill("---\nname: example\n---\nbody\n"),
      outside = path.join(fixture, "outside");
    fs.mkdirSync(outside);
    fs.writeFileSync(path.join(outside, "private.txt"), "PRIVATE_SENTINEL");
    fs.symlinkSync(
      outside,
      path.join(root, "linked"),
      process.platform === "win32" ? "junction" : "dir",
    );
    const { response, result } = call("inspect_skill", {
      root,
      entry: "SKILL.md",
    });
    expect(response.error.code).toBe("unsafe_path");
    expect(result.stdout + result.stderr).not.toContain("PRIVATE_SENTINEL");
  });
  it("enforces the negotiated resource limit and rejects executable frontmatter languages", () => {
    const root = skill("---\nname: example\n---\nbody\n");
    expect(
      call("inspect_skill", {
        root,
        entry: "SKILL.md",
        limits: { max_text_bytes: 4 },
      }).response.error.code,
    ).toBe("too_large");
    fs.writeFileSync(
      path.join(root, "SKILL.md"),
      "---js\nprocess.exit(77)\n---\nbody\n",
    );
    const { response, result } = call("inspect_skill", {
      root,
      entry: "SKILL.md",
    });
    expect(response.error.code).toBe("invalid_input");
    expect(result.status).not.toBe(77);
  });
  it("rejects malformed YAML without leaking parser details or source content", () => {
    const root = skill("---\nname: [PRIVATE_SENTINEL\n---\nbody\n");
    const { response, result } = call("inspect_skill", {
      root,
      entry: "SKILL.md",
    });
    expect(response.error.code).toBe("invalid_input");
    expect(result.stdout + result.stderr).not.toContain("PRIVATE_SENTINEL");
  });
  it("refuses unsupported operations instead of invoking the upstream CLI", () => {
    expect(call("init").response.error.code).toBe("unsupported_operation");
  });
});
