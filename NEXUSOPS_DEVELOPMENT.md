# NexusOps client development

This repository is the NexusOps fork of CC Switch. The implementation baseline is CC Switch `v3.20.1`, commit `3217f72596f2d1c0f879f0a05f83803825d9809f`.

- Origin: `https://github.com/HardieBao/nexusops-client`.
- Upstream: `https://github.com/farion1231/cc-switch`.
- Working branch: `codex/team-ai-control-plane`.
- Preserve upstream MIT licensing, author attribution and compatibility behavior.
- Follow this repository's pinned pnpm version and frozen lockfile; Rust uses its existing toolchain file.

Team business code belongs in `src-tauri/src/services/team/` and `src/features/team/`. Minimal module/command registration, feature selection, dedicated Team data, branding and update configuration are allowed. Do not redesign upstream provider models or modify the proxy to implement asset distribution.

The governing implementation task and server contract are in the NexusOps repository:

- `docs/tasks/260907-team-ai-control-plane-plan/task.md`
- `docs/tasks/260907-team-ai-control-plane-plan/decisions.md`
- `docs/openapi/assets.yaml`

Member desktop requests use `X-Nexus-Member-Key`. Profile and manifest are read-only; management operations require the web session and, where applicable, mandatory TOTP step-up. Never send member credentials to an arbitrary manifest URL or redirect destination. Do not record request headers or credentials in logs.

Store the Team connection credential in the operating system credential store, and store only its reference and non-secret state in Team metadata. An explicitly imported upstream provider and tool configuration still follow the upstream credential format; do not claim that those existing formats are encrypted. Preserve personal providers and files, show a preview before changing managed configuration, and retain recoverable file backups.

For development and tests, use `NEXUSOPS_CLIENT_TEST_HOME` and dedicated fixtures; upstream tests may still use the compatibility alias `CC_SWITCH_TEST_HOME`. Do not change HOME, USERPROFILE or a user's real Claude / Codex configuration to make a test pass. Read existing path overrides before launching the native app. The original CC Switch external UI test was stopped by automatic approval review; do not silently retry that blocked browser-to-app operation.

Server content protocol v1 uses normalized UTF-8 text and validated tar.gz Skill packages. Content hashes and archive hashes have different meanings. Shared cross-language fixtures reside in the Gateway server's `internal/asset/content/testdata/`; use them to validate the Rust implementation before calling it compatible.

Current Windows build environment: VS 2019 Build Tools MSVC 14.29.30133 with Windows SDK 10.0.19041, initialized using `VsDevCmd.bat -arch=x64 -host_arch=x64`. Newer installed toolchains lack desktop CRT libraries on this machine. This is a local build fact, not a universal requirement for contributors.

Build, API integration, file recovery, cross-platform installation, signing and updates require separate evidence. Source compatibility and unit tests do not prove OS deep-link dispatch or real model access.
