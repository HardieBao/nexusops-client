# Upstream maintenance for NexusOps Client

## Baseline and ownership

- Fork baseline: CC Switch `v3.20.1`, commit `3217f72596f2d1c0f879f0a05f83803825d9809f`.
- Upstream remote: `https://github.com/farion1231/cc-switch.git`.
- Fork remote: `https://github.com/HardieBao/nexusops-client.git`.
- Team code boundary: Cargo feature `team`; the upstream path must compile and test with `--no-default-features`.

A named maintainer with Rust/Tauri review responsibility must be assigned before a generally available client release. No person has been assigned in this repository yet, so owner approval and release rotation remain blockers. The release maintainer owns upstream checks, security triage, conflict resolution, and the release evidence index; a second maintainer reviews signing, migrations, and provider/config changes.

## Current upstream check

On 2026-09-07, `git fetch upstream --tags --prune` found no release tag newer than `v3.20.1`. The complete committed NexusOps fork was rebased onto the then-current untagged `upstream/main`, 38 commits after the release baseline. This exercises the maintenance process against newer source, but it does not label an unreleased upstream commit as a compatible CC Switch release.

### Complete-fork rebase drill

| Field | Result |
| --- | --- |
| Local branch | `codex/team-ai-control-plane` |
| Source commit before rebase | `53b46a11d99af31d33f8b7d378eba788812b4db7` on baseline `3217f72596f2d1c0f879f0a05f83803825d9809f` |
| Target | untagged `upstream/main` at `9692ff5e22327f0d2340d24271cf897f9d64fafd` |
| Merge base | `3217f72596f2d1c0f879f0a05f83803825d9809f` |
| Rebased feature commit | `cabd451605b26f53de923a014450750e60fa1808` |
| Verified code and package commit | `906af09f92bc4358d1b5f2d8c24a18517f8b8596` |
| Command | `git rebase upstream/main` in a separate worktree |
| Conflicts | `README.md`, `README_ZH.md` |
| Resolution | Kept NexusOps product, support, protocol and release-readiness copy; upstream CC Switch sponsorship and signed-release claims do not apply to the fork |
| Rebase timing | Automatic phase stopped on the two conflicts after 7.26 seconds; manual resolution time was not separately timed |
| Frontend checks | Frozen install, typecheck, format, renderer build, 137 files / 1094 tests passed |
| Rust checks | Team 47 passed / 3 ignored; full suite 2956 passed / 9 ignored with three exact Windows symlink-privilege filters; Team-on and Team-off checks passed |
| Release checks | Node release tests 9 / 9, actionlint 1.7.12, updater signature verification, Windows MSI and NSIS build passed |
| Windows smoke | Final release binary reconnected to the isolated Gateway and repeated a seven-asset no-change sync; an earlier MSI candidate installed and uninstalled per-user with isolated state |
| Decision | Preview only |

The rebase exposed and closed four compatibility gaps: Vitest had collected a Node-native release test, new upstream integration tests still targeted `.cc-switch`, `actions/stale@v10` received a removed input, and two app integration tests used a timeout too short under the full parallel suite. The release build also exposed mismatched Tauri JavaScript packages and an old CLI that could not patch the bundle-type marker with stripped symbols; the fork now pins compatible 2.10-line packages and Tauri CLI 2.10.0.

G3 remains blocked by the missing human maintainer and reviewer, platform signing, macOS/Linux/ARM installation evidence, and the two-version updater exercise. A tagged upstream release after `v3.20.1` still requires a fresh compatibility decision.

## Monthly check

Run this checklist during the first working week of each month and immediately for an upstream security release. This document deliberately does not create a scheduled task.

1. Fetch and inspect without changing the integration branch:

   ```bash
   git fetch upstream --tags --prune
   git log --oneline --decorate --no-merges <recorded-upstream>..upstream/main
   git tag --merged upstream/main --sort=-version:refname | head
   ```

2. Classify upstream changes that touch authentication, provider serialization, config paths, database migrations, updater logic, deep links, installers, Tauri dependencies, or platform-specific filesystem code as high attention. Security fixes are handled before routine monthly work.
3. Create `codex/upstream-YYYYMM-<version>` from the current reviewed fork commit in a separate worktree. Record the exact source and target commits before rebasing.
4. Rebase onto the selected upstream tag. Use an untagged `upstream/main` commit only for a documented preview drill; never call it release compatibility.
5. Resolve conflicts by preserving the NexusOps product identity, `nexusops://` scheme, separate data directory/database, GitHub updater endpoint, credential isolation, Team feature boundary, and personal-provider protection. Adopt upstream fixes outside those boundaries unless a test proves incompatibility.
6. Review every conflict and the final diff. Generated lockfiles may be regenerated; generated or vendored source is never hand-merged without its owning generator.
7. Run the checks below and attach logs. Merge only after a second maintainer reviews conflict resolutions and evidence.

## Required verification after a rebase

Run the commands from the repository root with the pinned toolchains:

```bash
corepack enable
corepack install
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --no-default-features --manifest-path src-tauri/Cargo.toml
cargo build --locked --no-default-features --manifest-path src-tauri/Cargo.toml
```

The two `--no-default-features` commands are the Team-off gate. They prove compilation and automated behavior for the upstream path; they do not replace the manual provider smoke below.

With Team disabled, use an isolated test home and verify that the existing provider list, add/import, explicit switch, live config write, restart, and rollback paths still work. With Team enabled, replay Key connection, profile preview, explicit provider import, stable asset install, local drift protection, remote A→B→A, disconnect, and credential cleanup. Confirm repeated imports do not duplicate providers and never rewrite personal providers.

Run `.github/workflows/build-evidence.yml` by manual dispatch on the rebased branch. Platform build success remains separate from installation evidence. Before release, repeat the real Windows, macOS, and Linux installation matrix in `RELEASE.md`.

## Conflict record template

Create one entry per drill or upgrade:

```text
Date:
Maintainer / reviewer:
Fork source commit:
Upstream tag and commit:
Merge base:
Rebase command:
Conflict files:
Resolution for each conflict:
Team-off checks:
Team-on automated checks:
Provider and asset smoke:
Platform build evidence:
Elapsed engineering time:
Known limitations / follow-up:
Decision: compatible / incompatible / preview only
```

Do not mark a newer upstream version compatible from a clean rebase alone. A version is compatible only after the Team-off and Team-on checks, provider/asset smoke, and affected platform builds pass against the rebased commit.
