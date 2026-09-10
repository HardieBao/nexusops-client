import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { splitFrontmatter } from "../../third_party/teamai/src/utils/frontmatter";
import {
  ruleFileExtensionForTool,
  ruleStemFromFilename,
} from "../../third_party/teamai/src/resources/rule-format";
import source from "../../third_party/teamai/source.json";

type Code =
  | "invalid_input"
  | "unsupported_operation"
  | "unsafe_path"
  | "too_large";
class Failure extends Error {
  constructor(public code: Code) {
    super(code);
  }
}
function fail(code: Code): never {
  throw new Failure(code);
}
const maximum = {
  max_text_bytes: 1048576,
  max_unpacked_bytes: 104857600,
  max_files: 2000,
  max_path_depth: 16,
  max_path_bytes: 240,
};
type Limits = typeof maximum;
type FileInfo = { path: string; size: number; sha256: string };
const decoder = new TextDecoder("utf-8", { fatal: true });

function object(value: unknown, keys: string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    fail("invalid_input");
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !keys.includes(key)))
    fail("invalid_input");
  return record;
}
function relative(value: unknown, limits: Limits): string {
  if (
    typeof value !== "string" ||
    !value ||
    path.isAbsolute(value) ||
    value.includes("\\") ||
    Buffer.byteLength(value) > limits.max_path_bytes
  )
    fail("unsafe_path");
  const parts = value.split("/");
  if (
    parts.length > limits.max_path_depth ||
    parts.some(
      (part) =>
        !part ||
        part === "." ||
        part === ".." ||
        /[<>:"|?*\x00-\x1f\x7f]/.test(part) ||
        /[ .]$/.test(part) ||
        /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part),
    )
  )
    fail("unsafe_path");
  return value;
}
function within(root: string, actual: string): void {
  const rel = path.relative(root, actual);
  if (path.isAbsolute(rel) || rel === ".." || rel.startsWith(`..${path.sep}`))
    fail("unsafe_path");
}
function text(bytes: Buffer): string {
  const result = decoder
    .decode(bytes)
    .replace(/^\uFEFF/, "")
    .replace(/\r\n/g, "\n");
  if (result.includes("\0") || /^---[^\s-]/.test(result)) fail("invalid_input");
  return result;
}

async function inspect(
  rootInput: unknown,
  entryInput: unknown,
  limits: Limits,
  skill: boolean,
) {
  if (typeof rootInput !== "string" || !path.isAbsolute(rootInput))
    fail("unsafe_path");
  const rootStat = await fs.lstat(rootInput);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail("unsafe_path");
  const root = await fs.realpath(rootInput);
  const entry = relative(entryInput, limits);
  if (skill && entry !== "SKILL.md") fail("invalid_input");
  const files: FileInfo[] = [];
  const seen = new Set<string>();
  let bytesRead = 0,
    visited = 0;
  let entryText: string | undefined;

  async function file(rel: string): Promise<void> {
    relative(rel, limits);
    const normalized = rel.normalize("NFC").toLowerCase();
    if (seen.has(normalized)) fail("unsafe_path");
    seen.add(normalized);
    if (files.length >= limits.max_files) fail("too_large");
    const location = path.join(root, rel);
    const stat = await fs.lstat(location);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1)
      fail("unsafe_path");
    within(root, await fs.realpath(location));
    const maximumBytes =
      rel === entry ? limits.max_text_bytes : limits.max_unpacked_bytes;
    if (
      stat.size > maximumBytes ||
      stat.size > limits.max_unpacked_bytes - bytesRead
    )
      fail("too_large");
    const handle = await fs.open(location, "r");
    try {
      const opened = await handle.stat();
      if (
        !opened.isFile() ||
        opened.ino !== stat.ino ||
        opened.dev !== stat.dev ||
        opened.nlink !== 1
      )
        fail("unsafe_path");
      const hash = createHash("sha256"),
        chunks: Buffer[] = [];
      const buffer = Buffer.alloc(65536);
      let size = 0;
      while (true) {
        const { bytesRead: count } = await handle.read(
          buffer,
          0,
          buffer.length,
          null,
        );
        if (!count) break;
        size += count;
        bytesRead += count;
        if (size > maximumBytes || bytesRead > limits.max_unpacked_bytes)
          fail("too_large");
        hash.update(buffer.subarray(0, count));
        if (rel === entry) chunks.push(Buffer.from(buffer.subarray(0, count)));
      }
      const after = await handle.stat();
      if (
        size !== stat.size ||
        after.size !== stat.size ||
        after.mtimeMs !== stat.mtimeMs ||
        after.nlink !== 1
      )
        fail("invalid_input");
      files.push({ path: rel, size, sha256: hash.digest("hex") });
      if (rel === entry) entryText = text(Buffer.concat(chunks));
    } finally {
      await handle.close();
    }
  }
  async function directory(rel: string, depth: number): Promise<void> {
    if (depth > limits.max_path_depth) fail("too_large");
    const location = path.join(root, rel);
    const stat = await fs.lstat(location);
    if (!stat.isDirectory() || stat.isSymbolicLink()) fail("unsafe_path");
    within(root, await fs.realpath(location));
    const entries = await fs.opendir(location);
    for await (const item of entries) {
      if (++visited > 4000) fail("too_large");
      const child = rel ? `${rel}/${item.name}` : item.name;
      relative(child, limits);
      if (item.isSymbolicLink()) fail("unsafe_path");
      if (item.isDirectory()) await directory(child, depth + 1);
      else if (item.isFile()) await file(child);
      else fail("unsafe_path");
    }
  }
  if (skill) await directory("", 0);
  else await file(entry);
  if (entryText === undefined) fail("invalid_input");
  const parsed = splitFrontmatter(entryText);
  if (parsed.raw && !parsed.valid) fail("invalid_input");
  const fallback = skill ? path.basename(root) : ruleStemFromFilename(entry);
  const name = parsed.data.name ?? fallback;
  const description = parsed.data.description ?? "";
  if (
    typeof name !== "string" ||
    !name.trim() ||
    name.length > 160 ||
    typeof description !== "string" ||
    description.length > 2000
  )
    fail("invalid_input");
  files.sort((a, b) =>
    Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)),
  );
  return { name, description, entry, files, content: entryText };
}

async function main(): Promise<void> {
  let requestId = "invalid";
  try {
    const chunks: Buffer[] = [];
    let size = 0;
    for await (const chunk of process.stdin) {
      const bytes = Buffer.from(chunk);
      size += bytes.length;
      if (size > 1048576) fail("too_large");
      chunks.push(bytes);
    }
    const request = object(JSON.parse(decoder.decode(Buffer.concat(chunks))), [
      "schema_version",
      "request_id",
      "operation",
      "input",
    ]);
    if (
      request.schema_version !== 1 ||
      typeof request.request_id !== "string" ||
      !/^[a-zA-Z0-9-]{1,80}$/.test(request.request_id)
    )
      fail("invalid_input");
    requestId = request.request_id;
    let data: unknown;
    if (request.operation === "version") {
      if (request.input !== undefined) fail("invalid_input");
      data = {
        upstream_commit: source.commit,
        upstream_version: source.package_version,
        operations: ["inspect_skill", "convert_rule", "version"],
      };
    } else if (
      request.operation === "inspect_skill" ||
      request.operation === "convert_rule"
    ) {
      const input = object(request.input, [
        "root",
        "entry",
        "target",
        "limits",
      ]);
      const limits = { ...maximum };
      if (input.limits !== undefined) {
        const supplied = object(input.limits, Object.keys(maximum));
        for (const key of Object.keys(supplied) as (keyof Limits)[]) {
          const value = supplied[key];
          if (
            typeof value !== "number" ||
            !Number.isSafeInteger(value) ||
            value <= 0 ||
            value > maximum[key]
          )
            fail("invalid_input");
          limits[key] = value;
        }
      }
      const skill = request.operation === "inspect_skill";
      if (
        !skill &&
        ((input.target !== "codex" && input.target !== "claude-code") ||
          typeof input.entry !== "string" ||
          !input.entry.endsWith(".md"))
      )
        fail("invalid_input");
      const inspected = await inspect(input.root, input.entry, limits, skill);
      if (skill) {
        const { content: _content, ...metadata } = inspected;
        data = { kind: "skill", ...metadata };
      } else
        data = {
          kind: "rule",
          ...inspected,
          extension: ruleFileExtensionForTool(
            input.target === "claude-code" ? "claude" : "codex",
          ),
        };
    } else fail("unsupported_operation");
    const response = JSON.stringify({
      schema_version: 1,
      request_id: requestId,
      ok: true,
      data,
    });
    if (Buffer.byteLength(response) > 4194304) fail("too_large");
    process.stdout.write(response + "\n");
  } catch (error) {
    const code = error instanceof Failure ? error.code : "invalid_input";
    process.stdout.write(
      JSON.stringify({
        schema_version: 1,
        request_id: requestId,
        ok: false,
        error: { code, message: code },
      }) + "\n",
    );
    process.exitCode = 1;
  }
}
void main();
