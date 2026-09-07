# NexusOps Client release guide

This repository is an MIT-licensed fork of [CC Switch](https://github.com/farion1231/cc-switch). CC Switch and its maintainers do not publish, sign, or support NexusOps Client builds. Keep `LICENSE` and `UPSTREAM_NOTICE.md` in source distributions and release assets. The Windows portable archive also contains both notices.

## Release status and support claims

The upstream `v3.20.1` documentation is the compatibility baseline. It lists Windows 10+, macOS 12+, and current Linux distributions such as Ubuntu 22.04+, Debian 11+, and Fedora 34+. The Tauri configuration also fixes the macOS minimum at 12.0.

Those upstream statements are inputs to testing, not evidence for this fork. NexusOps Client has no generally available release yet. A platform becomes supported only after one version and commit have passed installation, startup, `nexusops://` protocol import, Team connection, provider import, asset synchronization, exit, restart, and uninstall tests on that platform. Uninstall testing must confirm that existing user tool files remain present.

The current build matrix produces these candidates:

| Candidate       | Build runner                              | Current claim                                                                    |
| --------------- | ----------------------------------------- | -------------------------------------------------------------------------------- |
| Windows x86_64  | `windows-2022`                            | Build candidate; installation and Authenticode evidence required                 |
| Windows ARM64   | `windows-11-arm`                          | Build candidate; upstream minimum-version table does not establish fork support  |
| macOS universal | `macos-14`, arm64 and x86_64 Rust targets | Build candidate; release workflow requires Developer ID signing and notarization |
| Linux x86_64    | `ubuntu-22.04`                            | Build candidate; AppImage, deb, and rpm runtime tests required                   |
| Linux ARM64     | `ubuntu-22.04-arm`                        | Build candidate; AppImage, deb, and rpm runtime tests required                   |

`.github/workflows/build-evidence.yml` runs on pull requests and by manual dispatch. It performs frontend tests, Rust tests with Team enabled, Rust tests and a build with Team disabled, and a native release build without launching the GUI. It uploads the raw binary, toolchain record, SHA-256 list, and JSON provenance for 14 days. This workflow is a headless build gate. Its artifacts are not installers and do not prove code signing, notarization, installation, GUI behavior, or uninstall behavior.

## Fixed build inputs

- Node.js: `.node-version`, currently `22.12.0`.
- pnpm: the `packageManager` field, currently `10.12.3`; install with `pnpm install --frozen-lockfile`.
- Rust: `rust-toolchain.toml`, currently `1.95` with `rustfmt` and `clippy`.
- Tauri CLI: exact `2.10.0`; the core JavaScript API stays on the compatible 2.10 line, and each JavaScript plugin matches its Rust plugin minor version. All are resolved by `pnpm-lock.yaml` and `src-tauri/Cargo.lock`.
- Release runners: `windows-2022`, `windows-11-arm`, `macos-14`, `ubuntu-22.04`, and `ubuntu-22.04-arm`.
- Platform dependencies: each release candidate includes a hashed toolchain record with the hosted image version, Cargo, OS details, Xcode/Clang on macOS, Visual Studio/Windows SDK discovery on Windows, and resolved GTK/WebKit/libsoup packages on Linux.

This is an operationally repeatable build, not a bit-for-bit reproducible-build claim. Hosted runner images, Apple notarization, MSI metadata, archive timestamps, and native package tooling may change output bytes. Compare provenance first; investigate unexpected hash differences instead of assuming compromise or equivalence.

The release profile strips symbols. Tauri CLI versions before 2.10 can fail to patch the bundle-type marker in a stripped binary, which can break updater package detection. The pinned CLI includes the upstream fix described in [tauri-apps/tauri#14186](https://github.com/tauri-apps/tauri/issues/14186). Treat any `__TAURI_BUNDLE_TYPE variable not found` warning as a failed release build.

## Required release credentials

GitHub Actions secrets belong in the protected `release` environment. Limit environment approval to release maintainers. Stable promotion uses a second protected environment named `release-promotion`; configure required reviewers there and set its environment variable `NEXUSOPS_G3_PROMOTION_ENABLED=true`. Without that exact variable the promotion job fails closed. Never put the following values in source, workflow artifacts, logs, issue comments, or updater metadata.

| Name                                 | Purpose                                              | Required now               |
| ------------------------------------ | ---------------------------------------------------- | -------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Minisign private key for Tauri updater artifacts     | Yes                        |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the updater key                         | If the key is encrypted    |
| `APPLE_CERTIFICATE`                  | Base64 Developer ID Application certificate (`.p12`) | macOS release              |
| `APPLE_CERTIFICATE_PASSWORD`         | Certificate password                                 | macOS release              |
| `APPLE_ID`                           | Apple notarization account                           | macOS release              |
| `APPLE_PASSWORD`                     | App-specific notarization password                   | macOS release              |
| `APPLE_TEAM_ID`                      | Apple developer team                                 | macOS release              |
| `KEYCHAIN_PASSWORD`                  | Ephemeral CI keychain password                       | macOS release              |
| `GITHUB_TOKEN`                       | Create the prerelease and upload assets              | Supplied by GitHub Actions |

Windows Authenticode signing is not implemented in the current workflow and no Windows certificate is configured. This is a release blocker for a generally available Windows build. Adding it requires a protected certificate or hardware-backed signing provider, a timestamp service, and `Get-AuthenticodeSignature` verification before upload. The Tauri updater `.sig` proves updater payload integrity; it is not an Authenticode signature.

Linux packages are protected by the Tauri updater signature when delivered through the updater. Repository-native deb/rpm signing is not implemented. Do not describe those packages as distribution-signed.

The updater public key is committed in `src-tauri/tauri.conf.json`. The matching private key must exist only in the protected release environment and controlled offline backup. A lost key requires a client release signed by the old key before rotation; a leaked key requires stopping update distribution and a security response.

## G3 promotion evidence

Installation and update testing produces a reviewed JSON record named `G3-EVIDENCE.json`, which must be uploaded to the matching GitHub prerelease. It uses `schemaVersion: 1`, `decision: "passed"`, the exact tag and full 40-character commit, a reviewer, review date, controlled evidence location, and SHA-256 values for every `NexusOps-Client-<tag>-*` release payload.

Its `checks` object must mark each of these fields as `"passed"`: `windowsInstall`, `macosInstall`, `linuxInstall`, `protocolImport`, `teamConnectSyncDisconnect`, `uninstallPreservesUserFiles`, `windowsAuthenticode`, `macosSigningNotarization`, `windowsPathAndFileLock`, `macosPermissions`, `linuxLinksAndExecutableBits`, `validUpdate`, `invalidSignatureRejected`, `tamperedPayloadRejected`, `badUrlRecovery`, `interruptedDownloadRecovery`, `updaterUnavailableRecovery`, and `teamStatePreserved`.

The `artifacts` object maps each exact payload filename to its lowercase SHA-256. It includes installers, portable archives, Linux packages, and the macOS updater archive; it excludes `.sig`, build-evidence, checksum, toolchain, license, and notice files. Validate the completed record before upload:

```bash
node scripts/generate-build-evidence.mjs --validate-g3 G3-EVIDENCE.json vX.Y.Z FULL_COMMIT_SHA release-assets
```

The promotion workflow re-hashes every payload, verifies every updater signature against the public key embedded in the tagged `tauri.conf.json`, regenerates `latest.json`, checks the complete download set, and compares the regenerated updater manifest with the prerelease asset. Missing checks, changed artifacts, a mismatched commit, or an invalid signature stop promotion.

## Optional NexusOps R2 mirror

The desktop updater currently has one endpoint: this repository's GitHub Releases `latest.json`. `.github/workflows/sync-r2.yml` therefore cannot override or mask GitHub updates. An absent R2 configuration is a successful no-op. When configured, R2 synchronization requires the same G3 evidence, exact tagged scripts, complete download set, and valid updater signatures before it publishes root manifests.

To enable a future NexusOps-owned mirror, set all five values or none:

- Repository variables: `NEXUSOPS_R2_BUCKET`, `NEXUSOPS_R2_PUBLIC_BASE_URL` (HTTPS).
- Repository secrets: `NEXUSOPS_R2_ACCOUNT_ID`, `NEXUSOPS_R2_ACCESS_KEY_ID`, `NEXUSOPS_R2_SECRET_ACCESS_KEY`.

A partial configuration fails. No workflow contains the upstream `ccswitch.io` hostname, upstream bucket, or upstream R2 credentials.

## Candidate release procedure

1. Start from a reviewed commit with a clean tree. Confirm the G2 acceptance evidence and update `UPSTREAM_MAINTENANCE.md`.
2. Set the same semantic version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`; regenerate lockfiles where required.
3. Run the PR/manual build-evidence workflow. Retain every platform JSON file, `SHA256SUMS.txt`, toolchain record, command result, and workflow URL.
4. Create `vX.Y.Z` only after the release-environment credentials are present. The tag workflow validates identity/version equality, builds all configured candidates, verifies updater payloads against the public key embedded in the tagged client, records hashes and toolchains, verifies macOS signing/notarization, checks the complete download set, and publishes a GitHub **prerelease** with `latest.json` already attached.
5. Verify every uploaded hash against the per-platform evidence and inspect `latest.json`. It must contain six verified platform entries and URLs under `HardieBao/nexusops-client` for the same tag.
6. Run platform installation tests on the prerelease artifacts using real systems or controlled platform VMs. Record version, full commit, OS build, architecture, installer SHA-256, steps, result, and sanitized evidence location.
7. Exercise updates with two throwaway versions through an isolated HTTPS test endpoint or test repository using the same updater public key. GitHub's `/releases/latest/` excludes prereleases, so do not expose an unaccepted candidate as stable merely to test it. Record the temporary endpoint override and prove the production endpoint is restored in the candidate commit. Cover valid update, wrong signature, modified payload, bad/expired URL, interrupted download, restart, Team database migration, and preservation of provider and managed-asset state. A failed update must leave the installed version usable.
8. Create and review `G3-EVIDENCE.json`, upload it to the prerelease, then manually dispatch `Release NexusOps Client` with the exact tag. The protected `release-promotion` job is the only supported promotion path. Do not use the GitHub UI to flip the prerelease flag directly. Successful promotion is the point at which GitHub's `/releases/latest/` updater endpoint can expose it.

The tag workflow never creates a stable release. The separately approved promotion job does so only after the G3 evidence, payload hashes, updater signatures, and manifests pass. Repository administrators must preserve the environment and release-permission controls because GitHub administrators can bypass a workflow-only policy through the release UI.

## Stop distribution and recover

For a bad prerelease, leave it as a prerelease and publish a higher patch candidate. It is not selected by GitHub's latest-stable endpoint.

For a bad stable release:

1. Remove or draft the bad release so `releases/latest/download/latest.json` stops serving its updater metadata. Confirm the endpoint no longer resolves to the bad tag.
2. Preserve the release logs, hashes, manifest, commit, and incident record in controlled storage. Never replace assets under the same tag.
3. Determine whether the installed Team database and provider/asset state are backward compatible. Permit manual downgrade only with a tested schema and state path.
4. Publish a higher patch version from a reviewed fix, using new artifacts and signatures. Repeat installation and update tests before promotion.
5. If the optional R2 mirror was enabled, restore or remove its root `latest.json` only after the GitHub source of truth is correct. Versioned evidence remains immutable.

When update metadata is unavailable, the current installed version must continue to start and use its local state. Model calls and Team sync may still depend on server availability; updater failure must not erase local data or partially install a new version.
