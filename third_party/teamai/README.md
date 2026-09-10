# Selected TeamAI modules

Source: Tencent/teamai-cli, commit `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`, package 0.22.0.

`source.json` records Git blob IDs and SHA-256 for the original selected sources and MIT license. The build verifies these hashes before compiling. Do not format or edit the selected upstream files without updating the reviewed provenance and recording the patch.

- `src/utils/frontmatter.ts`: actual TeamAI YAML frontmatter parsing; used by Skill and Rule inspection.
- `src/resources/rule-format.ts`: actual TeamAI filename / per-tool extension rules; used by Rule conversion.
- `src/utils/logger.ts`: **NexusOps-owned shim**, not upstream logger code. It emits a fixed warning code to stderr; it never prints document content or paths to stdout.

The CLI entry point, init/pull, installers, Git providers, dashboard, transcript collector and session sharing are intentionally not linked into the worker. The worker is read-only. Credential-bearing HTTP and final canonical validation / installation remain in Rust.

This is a buildable worker, not proof of a complete desktop integration. The pinned Node preparation and x64 resource mapping are described in `../../scripts/teamai/README.md`. Tauri IPC integration and clean-machine package checks remain required by T11/T12/T21.
