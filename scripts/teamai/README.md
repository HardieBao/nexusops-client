# Windows TeamAI runtime

`pnpm teamai:prepare` compiles the pinned selected TeamAI modules, verifies their source hashes, and downloads the pinned Windows x64 Node executable and license at build time. Existing cache entries are reused only when their SHA-256 matches `runtime.json`. Downloads are bounded, time-limited, checked before replacement, and cannot follow redirects. No runtime download is performed by the installed client.

`pnpm teamai:test-runtime` runs the bundled executable by absolute path with no PATH, NODE_OPTIONS, home directory or application credentials in its environment. It verifies the executable / license / worker hashes, actual Skill and Rule conversion with Chinese and spaced input paths, unchanged source files, and denied filesystem writes and child process creation.

`pnpm build:teamai:windows` builds the x64 client using `src-tauri/tauri.teamai-x64.conf.json`. Its build hook prepares the runtime and renderer. Tauri resources map the executable, worker, manifest and license inventory to `teamai/` in the installed resource directory. A standalone EXE copied without this resource directory is not a complete TeamAI distribution. Existing other-platform builds do not gain x64-only resources.

The Rust `services::team::worker` module starts only this fixed worker with cleared environment, read permissions limited to its resource directory and selected input root, hidden window, bounded streams, timeout and cancellation. Node permissions are defense in depth for trusted bundled code, not an OS sandbox against arbitrary malicious code. In particular, Node 24 filesystem / child-process permissions are not a general network sandbox. The selected worker includes no network client and receives no credentials.

After `pnpm teamai:prepare`, run the actual Windows Rust tests in the project's MSVC environment:

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::worker -- --include-ignored
```

These runtime tests are explicitly ignored by default because they require the prepared Windows executable; report the explicit test result separately. The `teamai_run` Tauri command is registered and shares the Team cancellation signal and operation lock. Asset-import UI wiring, candidate validation / authorization, server publication, clean-machine installation, and UAT acceptance remain required by task.md.
