# NexusOps Client

[简体中文](./README_ZH.md)

NexusOps Client is a desktop client for team AI access and managed assets. It connects a developer workstation to a NexusOps Gateway while retaining selected local provider, MCP, prompt, and skill management capabilities inherited from CC Switch.

> **Development status:** this repository does not yet claim a signed public release or verified installer support for Windows, macOS, or Linux. Build and installation evidence is tracked separately. Do not use CC Switch release packages as NexusOps Client packages.

## Current scope

- Connect to an organization workspace with a Gateway URL and member key.
- Preview team provider changes before importing them. Importing a provider does not automatically make it current.
- List authorized Stable assets and apply supported updates with integrity and local-drift checks.
- Retain upstream local configuration features where the fork has not replaced their storage formats.

The presence of an adapter or preset in source does not prove that a provider, model response, or tool integration has passed end-to-end testing. Check the project acceptance evidence before making compatibility claims.

## Protocol and installation isolation

NexusOps Client uses:

- application name: `NexusOps Client`
- application identifier: `io.nexusops.client`
- deep-link scheme: `nexusops://`

The application does not register or accept `ccswitch://`. That scheme remains owned by an independently installed upstream CC Switch application, so both applications can keep separate protocol handlers.

On Linux, the Tauri plugin checks `x-scheme-handler/nexusops` and uses `nexusops-client-handler.desktop`. Source and configuration tests verify this separation; an installed side-by-side smoke test remains part of platform release validation.

The current configuration root is `~/.nexusops-client`. The Cargo package and library names remain `cc-switch` and `cc_switch_lib`; the desktop binary is explicitly named `nexusops-client` so Linux creates `nexusops-client-handler.desktop` instead of competing for CC Switch's handler filename. Database and log filenames such as `cc-switch.db` and `cc-switch.log`, remote-sync defaults, and other compatibility fields retain upstream names where renaming would require a separate data migration or widen merge risk. These internal names do not register the upstream URL scheme and are not the product name.

The legacy `flatpak/` manifest is not a NexusOps Client release target and still carries upstream identifiers. It must be branded and verified separately before this project claims Flatpak support.

## Development

Requirements include Node.js, pnpm `10.12.3`, the Rust toolchain pinned in [`rust-toolchain.toml`](./rust-toolchain.toml), and the platform dependencies required by Tauri.

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo test --manifest-path src-tauri/Cargo.toml
```

Additional project documents:

- [Team usage guide](./TEAM_GUIDE.md)
- [Development notes](./NEXUSOPS_DEVELOPMENT.md)
- [Release requirements](./RELEASE.md)
- [Security policy](./SECURITY.md)
- [Support](./SUPPORT.md)

## Upstream and license

NexusOps Client is an independent fork based on CC Switch `v3.20.1`. CC Switch and its maintainers do not publish, sign, endorse, or support this fork. Upstream compatibility is a maintenance goal, not a guarantee, and upstream promotions or partner offers do not imply a NexusOps relationship or eligibility.

See [UPSTREAM_NOTICE.md](./UPSTREAM_NOTICE.md) for the pinned upstream revision and attribution. The inherited code is distributed under the MIT License; see [LICENSE](./LICENSE).
