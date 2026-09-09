# Tencent TeamAI 融入 NexusOps Client 的可行性

结论：可行，建议选择能力接入，而不是把完整 CLI、Dashboard 和现有 Team 模块同时运行。以 NexusOps 的组织权限、资产发布、凭据和管理员统计为统一入口。

本次为源码与契约评估，未执行 TeamAI 的初始化、拉取、配置安装或真实工具验收。检视基线是 `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`，package version `0.22.0`，核对时间 2026-09-10。

## 已核实的能力

| 能力 | 上游证据 | 适合在客户端中的位置 |
| --- | --- | --- |
| Skills / Rules / Agents / Hooks / MCP 分发，多工具映射 | README、资源处理模块 | 资产库、工具接入 |
| 角色、标签、跨仓库来源 | `roles / tags / source` 命令 | 资产订阅、资产源管理；授权仍由 NexusOps 服务端决定 |
| Git-free HTTP 接入 | `init --http`、HTTP 集成测试、`local-agent.ts` | 团队成员连接流程，无需让每位成员拥有 Git 仓库写权限 |
| 设备 / 工具状态 report、资源 sync、执行 ack | `local-agent.ts` | 工具状态、资产分发及执行结果反馈 |
| 团队知识 recall / learnings / teamwiki | README，标记 beta | 后续新增「团队知识」，默认可选 |
| 会话、Token、干预信号、团队摘要 | dashboard / session / stats 相关模块，标记 beta | 独立的本机分析能力；不能直接替换已约定的最小统计上报 |

README 宣称支持多个工具，不等于在 NexusOps 环境中已验证。Codex / Claude Code 优先，其余工具按版本逐个验收。

## 与现有系统的分工

- NexusOps Gateway / 管理后台继续负责组织、员工、权限、Key、额度、资产 revision、canary / stable、审批、回滚和管理员统计。
- 现有 Client 负责窗口与页面、OS 凭据存储、配置预览、显式启用、资产哈希校验、本地漂移保护及恢复。
- TeamAI 作为适配能力来源，优先引入多工具资源格式、知识检索和 HTTP 分发协议思想。Git 仓库作为可选的资产导入来源，不成为另一套成员身份和发布授权系统。
- SkillOps 与 TeamAI 的 Hook 收敛到一个安装与事件分发入口；同一事件不能由两套 collector 各自上报一次。

## 最有价值的接入点：HTTP 模式

当前源码已有以下默认端点，并允许配置路由覆盖：

| TeamAI 端点 | 语义 | NexusOps 接入要求 |
| --- | --- | --- |
| `/api/projects/mine` | 用户项目列表 | 明确项目与当前组织 / 工作区的关系；不能把本地路径当可信授权主体 |
| `/api/local-agent/report` | 工具、设备与资源清单上报 | 明确定义新增字段及开关，服务端从凭据绑定真实员工 |
| `/api/local-agent/sync` | 拉取资源与指令 | 只下发当前授权的 stable revision，复用哈希、冲突和权限规则 |
| `/api/local-agent/commands/ack` | 执行结果确认 | 指令 ID、幂等、过期、重试和结果语义独立定义 |
| `/api/local-agent/get-config` | 配置读取 | 限定配置类型，避免静默修改个人模型、MCP、插件或 Hook |

这不是简单改 base URL 就能接入。TeamAI 使用 Bearer 与 `X-API-Token`，现有客户端使用 `X-Nexus-Member-Key`；payload、返回结构、错误码、项目绑定和同步语义也不同，需要一个经过测试的兼容层。普通成员不能借兼容接口获得资产发布或管理员权限。

## 必须解决的冲突

### 资产只允许一个实际写入者

两套同步器都可能操作 `.claude`、`.codex`、Skills、Rules、MCP 和配置文件。NexusOps 的 hash / revision / drift 记录与 TeamAI 的安装 manifest 不能互不知情地覆盖同一路径。

建议第一版由现有 NexusOps 安装器执行实际文件操作；TeamAI 提供资源发现、格式转换或导入来源。经审批的资产仍走 NexusOps stable 通道，不能用每次 SessionStart 自动 Git pull 绕过审批与本地冲突保护。

### 采集和上报的数据边界不同

当前 NexusOps 事件只有 `id / runtime / event / timestamp`。TeamAI 的 `dashboard-collector.ts` 会提取提示词前 200 字作为本地摘要，并处理工作目录、transcript 和部分助手输出；不能因此沿用“整个 TeamAI 采集都只有四个字段”的说法。

HTTP `buildReportPayload` 还包含主机名、OS、工具版本、资源清单，以及被选择工作区的路径、名称和项目 ID。它们不是提示词正文，但同样超出现有最小统计契约。

接入时默认保留 NexusOps 的当前上报白名单。若需要设备名、技能名、Token、工作区等新维度，逐字段定义目的、来源、保存期限、组织隔离、用户可见说明和授权开关；不能通过直接转发原始 TeamAI payload 隐式扩容。

TeamAI 的 `session save` 默认会保存本机日志；团队推送需要 `--push`，推送摘要中的提示词需要额外 `--include-prompt`。本地收集、HTTP 上报与 Git 贡献是三条不同路径，不能混为一谈。

### 原生桌面与 Node CLI 的交付差异

TeamAI 为 TypeScript / Node CLI，发布入口 `dist/index.js`，tsup 构建目标为 `node20`；包中没有稳定的公开 SDK exports，也没有可直接当作桌面 IPC 使用的统一 JSON 输出契约。

建议将需要保留的 Node 能力封装为受控子进程，以固定版本、结构化请求 / 响应、超时与取消对接 Rust。打包运行时、WASM 与资源文件时需要按 Windows / macOS / Linux 验证。员工无需全局安装 npm 包，也不需要使用终端。不要通过抓取带颜色的 CLI 日志驱动产品状态。

已有原生 Hook 采集和安全安装逻辑继续复用；不建议立即把整套 TeamAI 代码重写为 Rust，也不建议把整个 TeamAI Dashboard iframe 嵌入 Tauri。

## 融合后的页面设计

原先工作台方案可继续使用，新增能力放进已有任务结构：

- 工作台：真实工具状态、同步摘要、待处理冲突。
- 工具接入：统一的 Codex / Claude Code 等适配器、配置预览和 Hook 状态。
- 资产库：NexusOps 已发布资产、个人资产、可选 Git 资产源。
- 使用记录：本机请求统计；组织上报状态独立展示。
- 团队知识：在完成最小集成后再启用 recall、知识浏览和经验草稿；提交到团队前预览。
- 组织与设置：单一组织身份、采集授权、资产源和高级设置。

组织管理员仍在网页后台完成成员、策略、资产审批和统计管理。

## 推荐实施阶段

1. 先完成现有 Client 的导航与任务拆分，给扩展能力留出稳定入口。
2. 用固定 TeamAI 版本做隔离适配实验：只支持 Codex / Claude Code、只读资源，验证双方 Hash / scope / path 语义，不自动安装第二套 Hook。
3. 接入经过收敛的 HTTP 契约：鉴权、stable 资源、去重 ack、撤权、离线重试、取消和回退。
4. 增加知识检索与经验草稿；Git 贡献、自动知识提取和新增遥测字段作为独立能力开启。

最小验收必须证明：已有个人配置不被破坏；同一 Hook 事件不重复计数；被撤销的 Key 不能同步 / 上报；不同员工和组织隔离；本地编辑不被静默覆盖；关闭集成后可移除自有 Hook 和凭据而保留用户文件。

## 许可证与来源

仓库 LICENSE 明确写明 MIT 且没有附加限制。引入或改编代码时保留 Tencent 版权与 MIT 许可文本，并核对实际打包的依赖许可证。

- [README 与能力矩阵](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/README.md)
- [LICENSE](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/LICENSE)
- [package.json](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/package.json)
- [CLI 入口](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/index.ts)
- [HTTP 协议与上报 payload](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/local-agent.ts)
- [HTTP 模式集成测试](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/__tests__/http-repo-integration.test.ts)
- [本机会话采集](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/dashboard-collector.ts)
- [会话保存及团队推送](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/save-session.ts)
