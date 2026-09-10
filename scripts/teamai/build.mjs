import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { build } from "esbuild";

const root = fileURLToPath(new URL("../../", import.meta.url));
const vendor = path.join(root, "third_party/teamai");
const source = JSON.parse(
  await fs.readFile(path.join(vendor, "source.json"), "utf8"),
);
for (const file of source.selected_files) {
  const bytes = await fs.readFile(path.join(vendor, file.path));
  if (createHash("sha256").update(bytes).digest("hex") !== file.sha256)
    throw new Error(`TeamAI source hash mismatch: ${file.path}`);
}
const outdir = path.join(root, ".teamai-build");
await fs.mkdir(outdir, { recursive: true });
const result = await build({
  absWorkingDir: root,
  entryPoints: ["scripts/teamai/worker.ts"],
  outfile: path.join(outdir, "worker.mjs"),
  platform: "node",
  format: "esm",
  target: "node20",
  bundle: true,
  legalComments: "eof",
  banner: {
    js: 'import { createRequire } from "node:module"; const require = createRequire(import.meta.url);',
  },
  metafile: true,
});
await fs.writeFile(
  path.join(outdir, "bundle-inputs.json"),
  JSON.stringify(result.metafile, null, 2),
);
const packages = new Map();
for (const input of Object.keys(result.metafile.inputs)) {
  if (!input.includes("node_modules/")) continue;
  let directory = path.dirname(path.resolve(root, input));
  while (directory !== path.dirname(directory)) {
    try {
      const metadata = JSON.parse(
        await fs.readFile(path.join(directory, "package.json"), "utf8"),
      );
      if (
        typeof metadata.name !== "string" ||
        typeof metadata.version !== "string" ||
        typeof metadata.license !== "string"
      )
        throw new Error("Missing package license metadata");
      packages.set(directory, metadata);
      break;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    directory = path.dirname(directory);
  }
}
await fs.mkdir(path.join(outdir, "licenses"), { recursive: true });
const dependencies = [];
for (const [directory, metadata] of packages) {
  const notices = (await fs.readdir(directory, { withFileTypes: true })).filter(
    (entry) =>
      entry.isFile() && /^(license|copying|notice)([.-].*)?$/i.test(entry.name),
  );
  if (!notices.length)
    throw new Error(`No license file for bundled package ${metadata.name}`);
  const files = [];
  for (const notice of notices) {
    const name = `${metadata.name.replace(/[^a-z0-9._-]/gi, "-")}-${metadata.version}-${notice.name}`;
    await fs.copyFile(
      path.join(directory, notice.name),
      path.join(outdir, "licenses", name),
    );
    files.push(`licenses/${name}`);
  }
  dependencies.push({
    name: metadata.name,
    version: metadata.version,
    license: metadata.license,
    files,
  });
}
dependencies.sort((a, b) => a.name.localeCompare(b.name));
await fs.copyFile(
  path.join(vendor, "LICENSE"),
  path.join(outdir, "LICENSE-TeamAI"),
);
const bytes = await fs.readFile(path.join(outdir, "worker.mjs"));
await fs.writeFile(
  path.join(outdir, "manifest.json"),
  JSON.stringify(
    {
      schema_version: 1,
      upstream_commit: source.commit,
      upstream_version: source.package_version,
      worker_sha256: createHash("sha256").update(bytes).digest("hex"),
      dependencies,
    },
    null,
    2,
  ),
);
console.log("Built verified TeamAI worker in .teamai-build");
