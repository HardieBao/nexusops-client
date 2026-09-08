# Security Policy / 安全策略

## Supported versions / 支持版本

Security support applies only to NexusOps Client versions that this repository explicitly marks as supported in its own GitHub Releases. Until a signed release and support matrix are published, development builds are not supported for production use; reports against the current default branch are still welcome.

安全支持只适用于本仓库在 GitHub Releases 中明确标为受支持的 NexusOps Client 版本。在签名版本和支持矩阵正式发布前，开发构建不作为生产版本提供支持；仍欢迎报告当前默认分支中的问题。

CC Switch releases are separate upstream products and are not NexusOps Client releases.

CC Switch 发布版属于独立的上游产品，不是 NexusOps Client 发布版。

## Security model / 安全模型

NexusOps Client is a local desktop application that runs with the permissions of the signed-in operating-system user. It manages local AI-tool configuration and can connect to an operator-supplied NexusOps Gateway.

NexusOps Client 是以当前操作系统用户权限运行的本地桌面应用。它管理本机 AI 工具配置，并可连接由用户或组织提供的 NexusOps Gateway。

The main trust boundaries are:

- `nexusops://` deep links and imported files are untrusted until parsed and confirmed. NexusOps Client does not register or accept the upstream `ccswitch://` scheme.
- Gateway profiles, manifests, asset archives, and API responses are remote input. A successful HTTP response is not sufficient to trust content or authorization.
- A Team connection key is intended for the operating-system credential store. Provider import may place a member key into the target tool's own configuration only after the user previews and confirms that action.
- The local HTTP proxy may be configured to listen beyond loopback. Every inbound request must therefore be treated as untrusted.
- WebDAV/S3 synchronization data, model-pricing data, provider icons, and other remote data remain untrusted input.

主要信任边界包括：

- `nexusops://` Deep Link 和导入文件在完成解析与用户确认前都属于不可信输入。NexusOps Client 不注册也不接受上游 `ccswitch://` 协议。
- Gateway Profile、Manifest、资产归档与 API 响应属于远端输入。HTTP 请求成功不足以证明内容或授权可信。
- Team 连接 Key 应存入操作系统凭据存储。只有用户预览并确认后，Provider 导入才可能把成员 Key 写入目标工具自身的配置文件。
- 本地 HTTP 代理可以配置为监听非 loopback 地址，因此所有入站请求都必须视为不可信输入。
- WebDAV/S3 同步数据、模型价格、Provider 图标及其他远端数据仍属于不可信输入。

The current configuration root is `~/.nexusops-client`. Compatibility markers such as `cc-switch.db`, `cc-switch.log`, remote-sync defaults, and internal crate names may remain in the fork to preserve data compatibility. They do not grant ownership of `ccswitch://` and do not mean that upstream maintains this application.

当前配置根目录是 `~/.nexusops-client`。为保持数据兼容，fork 内可能继续保留 `cc-switch.db`、`cc-switch.log`、远端同步默认值和内部 crate 名等旧标识。这些标识不会注册 `ccswitch://`，也不表示上游维护本应用。

## In scope / 范围内

- A complete path from an untrusted deep link, Gateway response, asset archive, sync payload, imported file, provider response, or proxy request to a security impact.
- Cross-organization access, authorization bypass, credential disclosure, unsafe archive extraction, path traversal, unintended command execution, or unconfirmed overwrites of personal configuration.
- Vulnerabilities in the build, signing, updater, or release chain for NexusOps Client artifacts.
- Remote executable content reaching the bundled renderer, an XSS path, or exposure of privileged IPC commands to a non-bundled origin.

- 从不可信 Deep Link、Gateway 响应、资产归档、同步载荷、导入文件、Provider 响应或代理请求抵达安全影响的完整利用链。
- 跨组织访问、授权绕过、凭据泄露、不安全解压、路径穿越、非预期命令执行，或未经确认覆盖个人配置。
- NexusOps Client 构建、签名、更新或发布链中的漏洞。
- 远程可执行内容进入内置渲染器、XSS 路径，或高权限 IPC 暴露给非打包来源。

## Out of scope / 范围外

- Direct IPC calls made only through DevTools or a locally modified frontend, without an untrusted-input path.
- Ordinary local file operations where the user chose both the path and content and no untrusted input participated.
- User-authored commands or integrations executing exactly as the user configured them. Hidden, truncated, or misrepresented imported commands remain in scope.
- Automated scanner output without a reproducible path and impact.
- Availability claims or bugs that apply only to an unsupported development snapshot, unless they also affect the current default branch.

- 只能通过 DevTools 或本地修改过的前端直接调用 IPC，且不存在不可信输入路径的问题。
- 路径和内容均由用户选择、没有不可信输入参与的普通本地文件操作。
- 用户自行编写的命令或集成按其配置运行。若导入命令被隐藏、截断或错误展示，仍属于范围内。
- 没有可复现路径和影响的自动扫描结果。
- 只适用于不受支持开发快照的可用性声明或缺陷；同时影响当前默认分支的情况除外。

## Reporting / 报告方式

Do not report vulnerabilities in a public issue. Use the [NexusOps Client private security advisory form](https://github.com/HardieBao/nexusops-client/security/advisories/new) and include the affected revision, untrusted input source, full data path, reproduction steps, and impact. Do not include real credentials or private organization assets.

请勿在公开 Issue 中报告漏洞。请使用 [NexusOps Client 私有安全公告表单](https://github.com/HardieBao/nexusops-client/security/advisories/new)，并附上受影响版本、输入的不可信来源、完整数据路径、复现步骤和影响。不要提交真实凭据或组织私有资产。

If a problem reproduces on an unmodified CC Switch release, report it separately through the upstream project's own security channel. CC Switch maintainers are not responsible for NexusOps-specific code or releases.

如果问题能在未经修改的 CC Switch 发布版中复现，请另行通过上游项目自己的安全渠道报告。CC Switch 维护者不负责 NexusOps 特有代码或发布版。

Response timing and disclosure coordination depend on maintainer availability and the verified impact; this document does not promise a fixed response or remediation SLA.

响应时间与披露协作取决于维护者可用性和经确认的影响；本文不承诺固定响应或修复 SLA。
