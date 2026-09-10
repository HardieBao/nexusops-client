// NexusOps-owned bridge shim. Upstream parsing warnings must not corrupt stdout.
export const log = {
  warn(_message: string): void {
    process.stderr.write('{"code":"frontmatter_warning"}\n');
  },
};
