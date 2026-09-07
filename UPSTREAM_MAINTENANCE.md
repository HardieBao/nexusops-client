# Upstream maintenance for NexusOps Client

## Baseline and ownership

- Fork baseline: CC Switch `v3.20.1`, commit `3217f72596f2d1c0f879f0a05f83803825d9809f`.
- Upstream remote: `https://github.com/farion1231/cc-switch.git`.
- Fork remote: `https://github.com/HardieBao/nexusops-client.git`.
- Team code boundary: Cargo feature `team`; the upstream path must compile and test with `--no-default-features`.

A named maintainer with Rust/Tauri review responsibility must be assigned before a generally available client release. No person has been assigned in this repository yet, so owner approval and release rotation remain blockers. The release maintainer owns upstream checks, security triage, conflict resolution, and the release evidence index; a second maintainer reviews signing, migrations, and provider/config changes.

## Current upstream check

On 2026-09-07, `git fetch upstream --tags --prune` found no release tag newer than `v3.20.1`. `upstream/main` resolved to `38cfafdc199604b132be5eafe3f5384d85124a81`, 32 commits after the fork baseline. Because this fork's Team implementation was still uncommitted during the check, a real rebase of the complete fork could not be performed without creating a misleading or incomplete maintenance result. This check is useful freshness evidence, not G3 rebase evidence.

The first complete maintenance drill must use a committed NexusOps feature branch and record the old fork commit, new upstream commit, merge base, conflict list, resolutions, elapsed time, and every check below. Until that drill passes, upstream compatibility and G3 remain open.

### Limited release-pipeline rebase drill

The independently committed release-pipeline slice was rebased on 2026-09-07 to exercise the mechanics without touching the in-progress Team implementation:

| Field | Result |
| --- | --- |
| Local branch | `codex/upstream-release-drill-20260907` |
| Source commit before rebase | `e874ad35` on baseline `3217f725` |
| Target | untagged `upstream/main` at `38cfafdc199604b132be5eafe3f5384d85124a81` |
| Rebased commit | `63827b91ece88e4b503e3e46bbd7855d2695cbfd` |
| Command | `git rebase upstream/main` in a separate worktree |
| Conflicts | None |
| Rebase duration | 1.13 seconds |
| Checks | actionlint 1.7.12 passed all three changed workflows; Node syntax passed; updater-manifest and build-evidence fixture tests passed |
| Decision | Preview only |

The release identity fixture was intentionally not run on the rebased slice because that isolated branch does not contain the uncommitted NexusOps package and Tauri identity changes. No Rust, renderer, provider, asset, installation, or updater runtime claim follows from this drill. A complete-fork drill remains required after the Team work is committed.

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
