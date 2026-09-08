# NexusOps Client

[English](./README.md)

NexusOps Client 是用于团队 AI 接入和受管资产同步的桌面客户端。它把开发者工作站连接到 NexusOps Gateway，同时保留从 CC Switch 继承的部分本地 Provider、MCP、Prompt 与 Skill 管理能力。

> **开发状态：** 本仓库目前不声明已经发布经过签名的正式版本，也不声明 Windows、macOS 或 Linux 安装包已经完成验证。构建与安装证据另行记录。不要把 CC Switch 的发布包当作 NexusOps Client 安装包。

## 当前范围

- 使用 Gateway 地址与成员 Key 连接组织工作区。
- 导入团队 Provider 前预览变更；导入不会自动切换当前 Provider。
- 查看已授权的 Stable 资产，并在完整性与本地漂移检查后应用受支持的更新。
- 在 fork 尚未替换存储格式的范围内，保留上游本地配置能力。

源码中存在适配器或预设，不等于相应 Provider、模型响应或工具集成已经通过端到端实测。对外描述兼容性前，应核对项目验收证据。

## 协议与安装隔离

NexusOps Client 使用：

- 应用名称：`NexusOps Client`
- 应用标识：`io.nexusops.client`
- Deep Link 协议：`nexusops://`

本应用不注册也不接受 `ccswitch://`。该协议仍由独立安装的上游 CC Switch 使用，因此两个应用可以保留各自的协议处理器。

在 Linux 上，Tauri 插件核对 `x-scheme-handler/nexusops`，并使用 `nexusops-client-handler.desktop`。源码与配置测试已验证这组隔离；安装包级别的同机冒烟仍属于平台发布验收。

当前配置根目录是 `~/.nexusops-client`。Cargo 包名和库名继续使用 `cc-switch` 与 `cc_switch_lib`；桌面二进制显式命名为 `nexusops-client`，使 Linux 生成 `nexusops-client-handler.desktop`，避免与 CC Switch 争用处理器文件名。`cc-switch.db`、`cc-switch.log` 等数据库和日志文件名、远端同步默认值及其他兼容字段在改名需要单独数据迁移或会扩大上游合并风险时保留原名。这些内部名称不会注册上游协议，也不是产品名称。

旧的 `flatpak/` manifest 目前不是 NexusOps Client 发布目标，仍带有上游标识。完成单独品牌化与验证前，本项目不声明支持 Flatpak。

## 开发

开发环境需要 Node.js、pnpm `10.12.3`、[`rust-toolchain.toml`](./rust-toolchain.toml) 固定的 Rust 工具链，以及 Tauri 在当前平台要求的系统依赖。

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo test --manifest-path src-tauri/Cargo.toml
```

相关文档：

- [Team 使用指南](./TEAM_GUIDE.md)
- [开发说明](./NEXUSOPS_DEVELOPMENT.md)
- [发布要求](./RELEASE.md)
- [安全策略](./SECURITY.md)
- [获取帮助](./SUPPORT.md)

## 上游归属与许可

NexusOps Client 是基于 CC Switch `v3.20.1` 的独立 fork。CC Switch 及其维护者不发布、不签名、不背书，也不为本 fork 提供支持。保持上游兼容是维护目标，不是保证；上游促销或合作伙伴信息也不代表 NexusOps 与相关方存在合作或用户具备优惠资格。

固定的上游版本与归属见 [UPSTREAM_NOTICE.md](./UPSTREAM_NOTICE.md)。继承代码按 MIT License 分发，完整文本见 [LICENSE](./LICENSE)。
