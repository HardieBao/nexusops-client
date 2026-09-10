import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";

const root = fileURLToPath(new URL("../../", import.meta.url));
const spec = JSON.parse(
  await fs.readFile(new URL("runtime.json", import.meta.url), "utf8"),
);
const outdir = path.join(root, ".teamai-build");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

// Only build-time downloads. The installed client never runs this script.
async function prepare(filename, artifact) {
  const destination = path.join(outdir, filename);
  try {
    const cached = await fs.readFile(destination);
    if (cached.length <= artifact.max_bytes && hash(cached) === artifact.sha256)
      return;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const response = await fetch(artifact.url, {
    redirect: "error",
    signal: AbortSignal.timeout(120000),
  });
  if (!response.ok || !response.body)
    throw new Error(`Runtime download failed: ${response.status}`);
  const chunks = [];
  let length = 0;
  for await (const chunk of response.body) {
    length += chunk.length;
    if (length > artifact.max_bytes)
      throw new Error("Runtime download exceeds size limit");
    chunks.push(chunk);
  }
  const bytes = Buffer.concat(chunks);
  if (hash(bytes) !== artifact.sha256)
    throw new Error(`Runtime integrity failure: ${filename}`);
  const temporary = `${destination}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(temporary, bytes, { flag: "wx" });
    await fs.rename(temporary, destination);
  } finally {
    await fs.rm(temporary, { force: true });
  }
}

await fs.mkdir(outdir, { recursive: true });
await prepare("node.exe", spec.executable);
await prepare("LICENSE-Node", spec.license);
const manifest = JSON.parse(
  await fs.readFile(path.join(outdir, "manifest.json"), "utf8"),
);
if (
  hash(await fs.readFile(path.join(outdir, "worker.mjs"))) !==
  manifest.worker_sha256
)
  throw new Error(
    "Worker integrity failure; rebuild before preparing the runtime",
  );
manifest.runtime = {
  version: spec.version,
  target: spec.target,
  executable: "node.exe",
  sha256: spec.executable.sha256,
  license: "LICENSE-Node",
  license_sha256: spec.license.sha256,
};
await fs.writeFile(
  path.join(outdir, "manifest.json"),
  JSON.stringify(manifest, null, 2),
);
console.log(`Prepared pinned Node ${spec.version} for ${spec.target}`);
