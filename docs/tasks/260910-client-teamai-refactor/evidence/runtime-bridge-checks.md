# Runtime bridge verification — 2026-09-10

Source: client branch `codex/client-teamai-refactor`, working tree after `fff87c34`; not a committed release artifact. Server baseline unchanged at `9a3022a0`. TeamAI source `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`.

| Check | Actual result |
| --- | --- |
| `pnpm teamai:prepare` | Exit 0; pinned Node executable and license hashes match |
| Repeat runtime preparation with fetch replaced by a throwing function | Exit 0; verified local cache works offline |
| `pnpm teamai:test-runtime` | Exit 0; 5 passed, 0 failed, 0 skipped |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::worker -- --include-ignored` | Exit 0; 5 passed, 0 failed, 0 ignored |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` | Exit 0 |
| `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --lib` | Exit 0 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | Exit 101; 2841 passed, 2 failed, 13 ignored |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Exit 0 |
| Changed build-script Prettier check and `git diff --check` | Exit 0 |

Rust commands used the baseline VS 2019 BuildTools `vcvars64.bat` environment. Full-suite failures both occur at symlink creation with Windows error 1314, in unchanged `codex_config` and `session_usage_grokbuild` tests. No test was removed or converted to a skip to obtain a passing summary. Detailed failure names are in acceptance.md.

Pinned Node 24.18.0 x64 EXE SHA-256: `9a4eb5f1c29c6a2e93852ead46b999e284a6a5ca8bab4d4e241d587d025a52de`.

Node LICENSE SHA-256: `148eacf7863ef4329224a29398623077200a27194aa075569faf4a0a85566ca5`.

Worker SHA-256: `659c90f48a480b0be8408fff52f0493810056d3e78f8f5089a01c72bcdf8ec96`.

The runtime checks launch the actual bundled executable by absolute path with only SystemRoot retained, convert actual fixtures, check their unchanged bytes, and verify permission denials. Native tests separately exercise timeout with a vanished PID, cancellation, output limits and response validation. These are not clean-machine installation, renderer IPC, server publication or UAT evidence. No final package has been declared accepted.
