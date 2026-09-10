# Rule native checks — 2026-09-11

Client branch: `codex/client-teamai-refactor`, working tree after `fff87c34`; not a final release SHA.

| Evidence | Actual result |
| --- | --- |
| Complete Rust lib suite | 2862 passed, 0 failed, 15 default ignored; exit 0 |
| Rule loaders, explicitly including installed CLIs | 6 tests passed at that checkpoint, including both real CLI cases |
| Later added rule / backup directory alias regressions | Passed in the final complete suite |
| clippy `--lib -- -D warnings` | Passed after the BOM correction |
| no-default-features lib check | Passed |
| Frontend unit suite | 146 files, 1167 tests passed |
| Frontend typecheck and renderer build | Passed; existing large-chunk warning remains |
| Browser fixture | Both activation paths visible; errors=[]; synthetic-data screenshots attached |

Installed CLI versions tested: Codex 0.153.2 and Claude Code 2.1.263. The input first passes through the pinned TeamAI worker (`6ae0619d067b1699bb2c6e435abf3ffe11a21d71`). The tests use isolated child configuration directories and local HTTP capture endpoints; they retain only marker checks, not composed prompts. Codex's deliberately rejected capture request is not a real model completion. Claude's local response is synthetic. Neither is UAT proof.

The native file integration covers both tools, including migration of an existing managed download, activation, unchanged replay, a manual edit, explicit overwrite, and restoration of that edit. Backups retain the per-rule state rather than copying an entire personal instruction document.

The final regression additionally confirms that recovery of a local restore does not promote a conflict receipt to an applied ACK. The final full-suite result remains 2862 passed, 0 failed, 15 default ignored; final clippy also passed.

Two failures were fixed before this result: Windows canonical backup paths were rejected by lexical parent comparison, and a UTF-8-BOM-only Codex override was mistakenly considered empty. Canonical directory checks retain link rejection, and override selection now matches the actual tested CLI.

Reproduction: prepare the runtime with `pnpm teamai:prepare`, supply absolute executable paths through `NEXUSOPS_TEST_CODEX_BIN` / `NEXUSOPS_TEST_CLAUDE_BIN` (plus `NEXUSOPS_TEST_GIT_BASH` when needed), then run `cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::rules -- --include-ignored` in the recorded MSVC environment.
