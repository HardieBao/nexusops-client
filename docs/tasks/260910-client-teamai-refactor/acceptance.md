# CLIENT-TEAMAI 验收进度

更新日期：2026-09-12。当前为实施中，不是最终完成报告。

原生交互补验：[Claude 提示词与托盘观察](evidence/prompt-and-tray-20260912.md)。提示词新建、取消、保存、启用、编辑、重启、禁用和删除均有实际文件 / 数据库结果；托盘图标与菜单可见，但菜单动作受自动化接口限制，已请求人工验证，尚未填为通过。

最新交付：[2fa03d89 Windows 包与原生回归](evidence/package-and-native-2fa03d89.md)，以及 [用户试用说明](trial-guide.md)。新包实际安装、10/10 原生页面回退 / 保留、内置 worker、快捷键与正常卸载通过。原生缺陷已完成新包复测，1192 项前端检查见 [保存页面迁移与隐藏工具回退](evidence/view-migration-and-fallback-20260912.md)。完整旧功能交互、托盘和 UAT 已认证场景尚未全部验收。

最新交付检查点：[Windows 标题栏与安装复测](evidence/windows-native-and-installer-20260912.md)。74eddefe 已实际安装、从安装目录调用内置 worker 并卸载，环境恢复核对通过。真实 Windows 标题栏关闭 / 单实例唤回通过；先前白屏确认为误选 WebView 内部关闭控件。深链确认与取消局部通过，托盘菜单仍待验收。[原生导出记录](evidence/native-delivery-20260912.md)保留早先观察。UAT 已部署的证据见 [部署记录](evidence/uat-deployed-20260911.md)，已认证完整验收仍待授权测试身份。下方较早检查点的数量和未部署说明仅代表当时状态。

最新 Windows 原生完整回归：2864 passed / 0 failed / 15 ignored。新增两项 CLI 加载测试默认忽略，但已显式执行通过。较早检查点的权限失败、BOM 入口选择及恢复路径问题均保留记录，不代表当前仍失败。

| 任务 | 状态 | 当前证据 |
| --- | --- | --- |
| T00 | 完成 | baseline.md：两个仓库基线、工具版本、成功重建客户端索引、前端 1105 测试 / typecheck、Rust Team 55 通过 / 4 跳过、Gateway 4 包检查和实际 UAT health |
| T01 | 完成 | feature-mapping.md：15 类旧 View、9 类工具上下文、快捷键 / 原生入口 / 管理阻塞都有迁移目标；实际回归留给 T10 |
| T02 | 完成 | contracts.md + contract-examples.json：v1 边界、字段、方法、上限、scope / 项目映射、候选与 ACK、状态阈值、隐私与正反样例明确 |
| T03 | 实施中 | 新 shell、导航、标题 / 操作与页面出口均已拆出；最新完整前端 1172 项测试通过。原生窗口 / 完整入口场景仍待后续验收 |
| T04 | 实施中 | Team 区块共享 controller，取消收尾、旧响应隔离和上报互锁已加入测试及界面证据；完整身份切换 / 原生故障场景仍需补齐 |
| T11—T12 | 实施中 | 固定模块、许可与 x64 运行时准备、Rust worker / Tauri 命令已实现；9 项 worker 测试、5 项运行时测试、5 项原生 worker 测试通过。完整安装包、前端调用与资源发布闭环仍未验收 |
| T14 | 实施中 | 五个接口与 transport 已接入原生 Skill 同步 / 补 ACK，clippy 已恢复通过；实际 Go HTTP + 数据库 + 客户端贯通及 UAT 联调仍需完成 |
| T15 | 实施中 | 工作台、手动与自动确认均已接入，混合资产共用执行器；前台抢占和只读状态刷新已验证。正式 Go / 客户端 / UAT 联合验收仍未完成 |
| T05—T10、T13、T16—T23 | 未验收 | 未缩小 task.md 的原目标；独立用量入口与首页只读摘要已先接入，但不代表对应完整任务已通过 |

## 自动 ACK 与可见状态（2026-09-11）

- 独立 60 秒后台循环仅确认已就绪记录；不调用文件安装 / 恢复函数，不阻塞原工具统计上传循环。前台操作拥有优先权，按 ID 释放操作槽，旧后台结束不能清除新前台状态；退出及状态销毁会取消后台请求。
- 无连接、连接不可用、同步锁忙、无到期记录均不发送。失败按持久化等待时间延后，协议失败也更新等待，身份错误会更新连接状态。界面只读 IPC 在资产页可见时刷新，离页停止。
- 原生两项操作所有权测试通过；既有真实文件故障测试改为后台完成最终 ACK，验证未提交 intent 不确认、忙碌跳过、最终不重新下载 / 改写文件。前端新增验证自动状态变化和离页停止轮询，定向 34 项通过。
- 完整检查：Rust 2864 passed / 0 failed / 15 ignored；前端 146 文件 / 1172 tests 通过；typecheck、renderer build、clippy、禁用 Team 构建通过。
- 浏览器实际 App 夹具模拟后台状态清空，等待提示随只读刷新消失；apply 1 次、手动 retry 0 次、状态读取 4 次，errors=[]。截图 `evidence/teamai-autoack-status.png` 为合成数据；真实后台发送由原生测试另行验证，不当作 UAT 证据。
- GitNexus 现有入口 impact 为 LOW，但新操作结构未完整入图；累计 detect_changes 为 CRITICAL（32 跟踪文件、155 符号、36 流程）。源码、所有权测试和完整回归共同补足覆盖。

## M1 证据

- 客户端起点 `f811e352`，实施分支 `codex/client-teamai-refactor`；主项目隔离工作树基线 `9a3022a0`。
- 前端基线：138 文件 / 1105 tests，0 failed；Rust Team：55 passed / 4 ignored，0 failed。
- 首轮新导航 / 首页 / App 回归：4 文件 / 39 tests，全部通过。
- 导航改动后的完整前端回归：141 文件 / 1135 tests，0 failed；typecheck 与 renderer build 通过。
- 实际 App + 原有 Update / Theme / Query 上下文的隔离 IPC 夹具：1280×800 首页与 960×640 工具页（深色）可见、无横向溢出，切换入口没有触发配置写入。全新浏览器会话记录 errors=[]。这不是原生 Tauri 验收，也不是服务端 / 真实工具联调。
- 界面截图：`evidence/shell-home-1280.png`、`evidence/shell-tools-960-dark.png`。临时夹具初次缺少 UpdateProvider 和构建 CSS，补齐与真实入口一致的上下文 / 编译样式后重测；没有为通过夹具修改业务错误处理。
- 本轮 shell 组件的 Impeccable 机械检查结果为 []。
- 定向测试首次发现测试步骤在首次运行说明对话框未关闭时查询后台可访问按钮；修正测试为真实点击确认后继续，保留原对话框行为，没有删除首次说明或放宽可访问性断言。
- App 的 JSX 保持原缩进层级，避免只因外壳包裹产生大范围格式噪声；当前产品差异以实际 git diff 为准。
- GitNexus upstream impact：App / getInitialView LOW，renderContent 的直接上游为 App。React 根渲染的图缺失由 main.tsx 源码和 App 交互测试补足。改动后 detect_changes 为 MEDIUM，涉及 App → 平台判断流程；没有据此宣称原生窗口已验收。
- 后续增量刷新失败：`Failed calling LOWER: Invalid UTF-8`。本轮仍保留源码、差异和测试覆盖证据，不能把失败后的索引当作最新完整图。

## 尚未满足的关键条件

TeamAI 固定模块、Windows x64 运行时准备、Rust 桥接、两工具 Rule 加载及工作台新同步入口已实现；最终安装包、管理员导入发布、实际 Go / 客户端联合验证未完成。自动 ACK 已接入；Team 页面完整身份切换、真实工具状态、独立本机聚合，以及新版本 UAT 全流程仍需完成。

## 统一同步界面与混合资产（2026-09-11）

- Codex / Claude 工作区的预览、刷新与同步已调用新原生命令；兼容回退仅处理明确的 upgrade_required，保留可见提示。认证 / 网络错误不降级，迟到的能力响应不会启动另一次旧协议请求。
- WeakMap 绑定预览对象与签发信息，版本检查后再执行；ACK 摘要按连接 / 工具隔离。界面显示安装结果和确认结果，手动重试只调用 teamai_retry_acks，切换工具不展示旧摘要。
- 新预览保留 legacy_items，普通资产按 ID / revision / hash 单独确认，但仍调用同一个 Rust 安装器。原生混合测试覆盖 Skill 与非启用 Prompt、普通资产版本变化拒绝、Skill 不能伪装为普通资产，以及 ACK 失败重试不重新下载。
- 旧网关测试夹具现在显式返回 upgrade_required；取消测试等到真实模拟请求已开始才取消，保留原来的进行中取消语义，没有靠延长超时通过。
- 前端 146 文件 / 1170 tests、typecheck、renderer build 通过；原生完整 2862 passed / 0 failed / 15 ignored，clippy 与禁用 Team 构建通过。
- 浏览器实际 App 的隔离夹具走完预览 → 混合同步 → ACK 等待 → 补确认。外部调用记录为 teamai_preview、teamai_apply、teamai_retry_acks，没有再次调用安装；errors=[]。截图为 `evidence/teamai-confirmation-pending.png` 与 `evidence/teamai-confirmation-complete.png`，包含明确的合成数据标记，不等于真实 UAT。
- GitNexus 共享工作区控制器为 HIGH；新原生模块仍有图覆盖缺口。累计 detect_changes 为 CRITICAL（32 跟踪文件、137 符号、36 流程），源码、定向 / 全量测试与实际 UI 检查共同验证。

## Rule 原生加载与恢复检查点（2026-09-11）

- 新增 Rule 上游适配：Codex 在实际生效 AGENTS 文件内管理单条规范区块，Claude 写入自己的规则目录；原托管副本路径不变，旧下载记录可升级。Rule 不转成 Codex 命令放行策略。
- 原文、个人内容、其他规范、Claude paths 条件均保留。大小上限、Codex 预算、管理标记、配置目录与备份链接边界有检查；修复 Windows 备份路径规范化前缀导致的误拒绝，同时仍拒绝目录别名越界。
- 安装器对“副本内容相同但入口未激活”的情况仍执行受保护提交；已完全同步才返回 unchanged。手工修改产生冲突，显式覆盖保存原入口备份，恢复只还原自己的规则内容。
- 中断恢复也区分“完成安装”与“恢复个人修改”：后者只写普通状态，不推进成功 ACK。两种工具的真实文件回归均验证 conflict 不会因此被提升为 applied。
- 真实文件集成测试覆盖 Codex / Claude 两种入口、旧 managed download 升级、重复同步、手工修改保护及恢复。新增两项别名拒绝测试验证规则目录和备份目录均不能被 junction / symlink 重定向。
- 两项显式 CLI 检查通过：先运行固定 TeamAI worker 转换 frontmatter Rule，再由原生适配器写入入口，启动 Codex 0.153.2 / Claude Code 2.1.263，在本机模型捕获端确认特定标记被加载。未保存完整提示词，未使用真实模型服务；这不是正式 UAT 或模型遵从性验收。
- BOM-only override 的实测曾失败：Codex 仍会读取该文件，导致写入 AGENTS.md 的规则被遮蔽。已修正入口选择并通过真实 CLI 复测，保留 BOM 原字节。
- 当前完整 Rust：2862 passed / 0 failed / 15 ignored；clippy 通过，禁用 Team 构建通过。前端：146 文件 / 1167 tests 全通过，typecheck / renderer build 通过；大分块告警仍留给性能验收。
- 实际 App 的隔离 UI 夹具验证两种工具的加载路径和支持说明，errors=[]。截图：`evidence/rule-activation-codex.png`、`evidence/rule-activation-claude.png`；截图为明确标注的合成数据，未点击执行安装。
- GitNexus 安装 / 恢复入口为 HIGH，累计影响为 CRITICAL，新模块图覆盖仍需源码及测试补足。没有将组件检查替代完整 Goal 完成条件。

## Windows 普通权限完整回归（2026-09-10）

- 两项目录别名测试在 Windows 使用真实 junction，Unix 保留 symlink。创建后额外核对 canonicalize 确实指向目标目录，原“越界必须拒绝”“循环只收集真实文件一次”断言未变；没有修改生产函数或 Windows 权限设置。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib symlink_`：7 passed / 0 failed / 0 ignored。
- 完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib`：2855 passed / 0 failed / 13 ignored，退出 0。显式忽略项仍须按适用范围分别验收，不因全量命令退出 0 而视为通过。
- 默认忽略清单已保存至 `evidence/native-ignored-tests.txt`。其中 Windows OS 凭据往返测试另行显式执行并通过：创建随机命名的合成凭据、读取、删除后确认不存在；没有读取或修改已有真实成员凭据。Worker 的三项实际运行时用例已有前述显式执行证据；真实 Gateway、S3、WSL、性能与本机语料用例仍按各自范围处理。
- GitNexus 两个测试符号 impact 均为 LOW；累计 detect_changes 仍为 CRITICAL（29 跟踪文件、103 符号、25 流程）。本轮变更仅测试夹具，生产行为未改；格式与 diff 检查通过。
- Rule 加载方式已依据官方文档记录在 [rule-loading.md](rule-loading.md)，为下一步原生适配提供实际入口和限制；尚未据此勾选 Rule 验收。

## 原生 Skill 安装与补 ACK 检查点（2026-09-10）

- 新增三个 Tauri 入口，调用真实 transport、共享同步计划与原 Rust 安装执行器。确认前重新核对用户预览的 command_id、asset_id、revision、kind 和 hash，过期 / 不同版本不进入文件安装。
- schema 3 保存 ACK intent，安装完成标记与资产 metadata 在同一 SQLite 事务中落盘；只有验证过磁盘 / 工具状态的明确提交入口可以推进成功标记。普通刷新 / 撤回标记 / 日志清理不推进 ACK，失败提交同时回滚 metadata 与 ACK 标记。
- 实际 Codex Skill 文件安装测试走 HTTP fixture → 原生预览 → 拒绝陈旧确认 → 安装 → 首次 ACK 503 → 重开 Team 状态库 → 等待期内不重试 → 续签 / 补 ACK。下载计数始终为 1，SKILL.md 字节及修改时间保持，队列最终清空。使用真实共享内容向量和原安装器；HTTP 服务仍为合成协议夹具，不等于 Go / UAT / 真实 CLI 验收。
- 首次故障注入发现恢复收尾重复保存会清空 retry_at，造成提前重试；已修复，且等待期未到时在发网络请求前返回。随后收紧普通 metadata 保存权限，新增回归证明它不能把 conflict 推进 applied。
- 最终 Team 定向检查：71 passed / 0 failed / 7 ignored；clippy `-D warnings` 通过，先前 16 项 dead_code 已通过实际生产调用消除。禁用 Team 功能构建通过。
- 最新完整 Rust lib 回归：2853 passed / 2 failed / 13 ignored；两项仍是已有符号链接权限 1314 问题，相关源文件未改。忽略项不视为通过；完整原生回归尚未达成。
- GitNexus 修改前 save_asset_state 为 CRITICAL（30 影响项），disconnect / repair_one_pending 为 HIGH；已读调用者并运行恢复回归。最终 detect_changes 为 CRITICAL（27 跟踪文件、98 符号、25 流程），新模块仍需结合源码 / 测试补足图覆盖。
- Rule 尚未接入工具原生加载位置，不能把 managed download 当安装成功；当前新 apply 明确拒绝该情形。查阅固定上游 [rules.ts](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/resources/rules.ts) 与 [types.ts](https://github.com/Tencent/teamai-cli/blob/6ae0619d067b1699bb2c6e435abf3ffe11a21d71/src/types.ts) 只能确认目录分发配置，不能替代实际 CLI 加载验证。此项仍是必须完成的原范围。

## Rust TeamAI transport 检查点（2026-09-10）

- 新增 `src-tauri/src/services/team/api/teamai.rs`：五个接口的严格 DTO、请求、作用域 / 版本 / 候选验证；复用现有 Client、单一敏感成员 Key 头、同源路径验证和受限响应读取。没有把凭据转交给 worker。
- 候选类型、工具、ID、哈希格式、30 分钟租期、5 分钟签发时钟偏差、到期、下载引用和完整选择集合均验证；未知指令 / 额外字段拒绝。命令 ID 与独立语言计算的有序 JSON tuple 测试值一致。
- 新增 Gateway 升级提示与限流错误；共用读取层不再将 429 当作临时故障立即重试，POST 不自动重放。调度层消费 retry_after_seconds 仍需 T15 接线，不能借 transport 代码声明已具备持久化重试。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::api`：15 passed / 0 failed / 0 ignored（7 项新 TeamAI 测试、8 项原 HTTP 回归）。使用本机真实 HTTP socket 的合成服务，覆盖协议往返、作用域、非法候选、旧 Gateway、重定向无凭据转发、限流不重放、超时和预取消；不是实际 Go Gateway / 数据库联调。
- 完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib`：2848 passed / 2 failed / 13 ignored。两项仍为原符号链接权限错误 1314，对应源文件未改；完整原生回归仍不算通过。
- **该检查点当时 clippy 未通过，现已由后续原生接线修复**：当时新增 transport 方法没有生产调用，`-D warnings` 报 16 处 dead_code；没有压制告警或伪造使用点，后续通过实际同步 / 补 ACK 调用消除。
- GitNexus 对现有 bytes_once / json / code 的影响分析为 LOW，但部分边缺失和 TeamError 同名歧义需要源码补足；修改后 detect_changes 为 CRITICAL（25 跟踪文件、72 符号、25 流程），新增未跟踪 transport 模块仍不具备完整图覆盖。Rust 格式与 diff 检查通过。

## Gateway sync / ack 持久化检查点（2026-09-10）

- 在同一隔离工作树继续实现 `POST /me/teamai/sync` 与 `/ack`，同步扩展成员认证白名单、BYOK 显式路径、能力响应和省略正文的审计配置。没有新增模型请求计费或使用事件字段。
- `CurrentMemberAssets` 的事务内查询抽成共享函数；原清单 / 下载仍保留 repeatable-read 事务，新指令签发与 ACK 在同一授权快照中执行。当前组授权、组织 / 工作区、用户、Stable 版本和哈希都参与检查。
- 新增 `0265_teamai_command_receipts.sql`，哈希 `0e25f0938d97cfe657facfc40bb277f68e4ceae89fb750ae866bc141967849ad`；更新迁移 ledger / head。沿用现有资产模块的 SQL repository，没有新建 Ent model 或跨库引用。
- 固定作用域元组产生 command_id；续签保留成功结果，不能降级成功 ACK；过期、撤权、旧 Stable、另一成员 / 工具确认均拒绝。重复 / 并发请求不产生第二份同一指令记录，冲突最多重试 3 次。
- 隔离 PostgreSQL 16 容器中，9 项实际仓储测试全部通过且无跳过，其中 5 项为 TeamAI 新测试；涵盖当前授权、续签、并发、隔离、过期、版本变化、200 / 201 边界与失败无部分写入。其余 4 项原资产仓储回归也通过。
- 另建空测试数据库运行既有迁移升级验证，从旧迁移集合升级到包含 0265 的当前集合，保留 ledger 且可重复启动；升级后查询确认 receipt 表存在。迁移 checksum 脚本通过。
- 最终 Gateway `go test ./...`、`go build -mod=readonly ./cmd/server`、`go vet ./...` 均退出 0；gofmt 与 diff 检查通过。默认全量测试中的环境依赖跳过不视为通过，上述实际数据库验证单独执行。
- GitNexus 仍未覆盖该工作树，新符号 impact 为 UNKNOWN / not found，detect_changes 返回 repository not found；没有用旧图证明新代码安全。已用直接调用者阅读、共享查询回归、真实 PostgreSQL 与完整 Go 检查补足。
- 测试容器为本任务隔离实例 `nexusops-teamai-test-20260910`，仅绑定本机 15439 端口、使用 tmpfs；未连接业务库、未部署 UAT。后续仍需真实 HTTP + 数据库 + 客户端安装 / ACK 故障恢复联调。

## Gateway 首批 TeamAI 接口检查点（2026-09-10）

- 源码位于主项目隔离工作树 `.worktrees/team-ai-control-plane`，分支 `codex/team-ai-control-plane`，基线 `9a3022a0` 后的未提交修改；主工作树与 UAT 没有因此升级。
- 新增 `GET /api/v1/me/teamai/projects`、`GET /api/v1/me/teamai/config`、`POST /api/v1/me/teamai/report`。复用成员 Key、IP / 分组检查、角色降权、BYOK 显式路径、工作区租约、限流与审计；不允许完整 CLI 远程指令或同步写入。项目身份来自部署 scope 和已认证用户，拒绝客户端指定另一成员。
- 资产 GET、既有工具上报与新 TeamAI 接口分成明确的认证模式，保留既有路由方法边界。混用 Authorization / x-api-key / x-goog-api-key / X-API-Token 均拒绝，空身份头也不能绕过检查；新 TeamAI 的停用成员返回 403，过期 / 停用 Key 返回 401。
- report 只确认协议，不写模型费用或工具事件；严格限制版本 / runtime / 额外字段和 16 KiB 输入。补充真实审计服务写入测试，证明被拒绝正文不会进入审计存储。
- 六个新增 Go 测试函数包含双组织 / 双成员作用域、方法与凭据反例、BYOK / 租约路径边界、审计正文省略，以及 httptest 实际 TCP HTTP 服务的 12 个接口场景。HTTP 测试使用真实注册函数 / 中间件 / handler / 应用层，Key 存储为合成夹具；不是 UAT / 真实数据库身份验证。
- 最终 `go test ./...`、`go build -mod=readonly ./cmd/server`（输出到临时检查 EXE）、`go vet ./...` 均退出 0；gofmt 与 `git diff --check` 通过。未新增迁移，本轮未部署。
- GitNexus 旧索引为 54ac6be，SetupRouter / 工作区租约判断 impact 为 HIGH；共享成员 Key 函数在旧图中缺失。保留指令文件的 `analyze --index-only --workers 4` 进程结束后无完成记录，status 仍旧；指定实际工作树的 detect_changes 返回 repository not found。索引刷新及范围检测没有成功，不将旧主工作树的图当作本轮完整证据；通过调用源码、实际 diff、完整 Go 检查补足。后续仍需恢复索引覆盖。
- T14 继续实施：sync 候选、持久化签发 / ACK、撤权与重试幂等、客户端 transport 和 UAT 联调尚未完成。

## Windows 运行时与原生桥接检查点（2026-09-10）

- 新增 `scripts/teamai/runtime.json` 固定 Node 24.18.0 的官方 Windows x64 EXE 和 LICENSE 校验值；`teamai:prepare` 只在构建时下载、限时限量读取并在替换前验证哈希。禁用 fetch 后复用已校验缓存也通过。
- `build:teamai:windows` 使用独立 x64 Tauri 配置，映射完整运行资源和许可到 `teamai/`。尚未将配置存在当作最终安装包验证；原 ARM64 等构建不混入 x64 运行时。
- `pnpm teamai:test-runtime`：5 passed / 0 failed / 0 skipped。实际内置 EXE 在无 PATH 的环境执行 TeamAI，中文空格路径 Skill / Rule 转换保持源字节不变，未授予的文件读写与子进程操作被拒绝。
- Rust `Worker` 使用固定资源目录、清理环境、限制管道大小、默认 15 秒超时，并在错误 / 取消时 kill + wait；`teamai_run` 注册到 Tauri 且复用 Team 同步锁及取消信号。renderer 不能指定启动命令或业务凭据。
- 显式执行 `cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::worker -- --include-ignored`：5 passed / 0 failed / 0 ignored；包括真实 worker 调用、中文输入、超时后 PID 已消失、取消和诊断超限。常规测试默认跳过的 3 个 Windows 运行时用例在此处实际执行，并非将忽略当通过。
- 首次原生编译因未载入 VS 2019 环境缺少 MSVC 头文件而失败；切回基线工具链后继续。真实测试又复现 Node 不接受 Windows verbatim 模块路径，使用 dunce 规范化兼容路径修复。生命周期探针缺失 SystemRoot 导致 Node CSPRNG 初始化退出，已补上与实际启动器一致的最小 OS 环境。最终上述 5 项测试均通过，原失败记录不隐去。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings`、`cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --lib`、Rust 格式检查均通过。
- 完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib`：2841 passed / 2 failed / 13 ignored。失败为既有 `codex_config::tests::resolve_catalog_rejects_symlink_escaping_config_dir` 和 `services::session_usage_grokbuild::tests::symlink_cycle_does_not_cause_stack_overflow`，均在创建 symlink 时收到 Windows 1314 权限错误；两个源文件本轮无 diff。未删除 / 忽略这两个测试，完整原生回归仍未通过，须在具备符号链接权限的环境补验。默认忽略的 3 个新 worker 测试已另行显式通过，其余忽略项不据此视为通过。
- 修改后 GitNexus detect-changes：23 个跟踪文件、57 符号、25 流程，CRITICAL；新增未跟踪模块的图覆盖不足仍由实际源码和运行测试补足。`git diff --check` 与新增构建脚本格式检查通过。
- 本检查点不证明 Windows 安装资源定位、前端实际 IPC、完整候选验证 / 发布、真实工具和 UAT 联调已经通过。T11 / T12 保持实施中，最终按 S19—S21、S29—S30 验收。

已读到 UAT 基线版本 9a3022a0，不等于本次重构已部署。最终必须逐项满足 task.md 的 O1—O6、M1—M5 和 S01—S30。

## 页头与 Team 工作区拆分检查点

- 从 d02fb7d4 继续：`ClientHeader` 将运行时状态、管理状态与操作回调分开，`ClientPageOutlet` 管理原页面过渡；原业务处理和原生窗口函数保留在 App。顶部统计快捷入口进入独立 usage 页面。
- Team 拆为 `useTeamWorkspace`、`TeamWorkspaceBoundary`、四个任务区块与反馈组件。organization / providers / assets 各自显示相关内容；旧 TeamPage 默认 all 仍用于兼容原调用与回归。只有组织页面显示上报设置。
- 全局 Team 边界仅在 feature flag 开启时挂载，页面切换不重新创建 controller；初次同步使用当前受支持工具，修复了原先硬编码 codex 的偏差。
- 新测试复现并验证了旧的本机状态返回晚于显式新连接时的保护：旧响应不能覆盖新连接，也不再触发额外的初始同步请求。初始恢复请求也检查版本，并处理第二次读取失败。
- 65 项定向测试通过；完整前端回归 143 文件 / 1146 tests 通过；typecheck 和 renderer build 通过。Rust 与服务端本轮没有修改，没有把旧 Rust / UAT 证据作为新实现通过依据。
- 实际 App 的隔离 IPC 浏览器夹具走过首页 → 组织 → 资产库 → 团队资产，`team_status` 和初始 `team_preview_sync` 各 1 次，配置写入 0 次，页面 errors=[]；960 宽度下无横向溢出。截图为合成组织 / 空资产数据，不能替代真实安装或 UAT。
- 截图：`evidence/team-organization-1280.png`、`evidence/team-assets-960.png`。工具配置和组织资产不再挤在同一 Team 页面里。
- Impeccable 机械检查针对本轮拆出的组件返回 []；GitNexus 修改前索引确认 d02fb7d4，App / renderContent / TeamPage impact 为 LOW，React 调用覆盖仍结合源码和测试判断。
- 原 TeamPage 的 11 个主要操作函数与新 controller 做了 TypeScript token 对比，连接、Profile 导入、Provider 操作、同步、恢复等函数内容保持一致；初始读取的工具选择和请求版本保护另有显式修改与测试。
- 修改后索引增量刷新再次失败：`Failed calling LOWER: Invalid UTF-8`。已记录，不能将该索引作为完整的新模块影响图；本次验收依靠源码对比、类型检查、全量前端测试和隔离界面证据。

## 取消收尾与 TeamAI worker 检查点（2026-09-10）

本检查点对应 `fff87c34` 之后的工作树修改，尚未代表发行版本。T03 / T04 继续实施，T11 / T12 开始实施，其余原验收范围保持不变。

- 取消操作先发信号，等待当前前端请求及 Rust `team_wait_idle` 写入锁收尾，然后读取本机连接和恢复历史；旧响应失效，已完成写入不会被误报为回滚。取消失败与本机结果不确定分别显示固定提示，用户主动刷新后继续。
- 组织上报操作共享忙碌状态；刷新 / 取消过程中不可并发启用采集，正在上报时不提供虚假的取消按钮。
- 本轮实读完整前端日志：146 文件 / 1167 tests 全通过；重新运行 `pnpm typecheck` 与 `pnpm teamai:typecheck` 成功。renderer build 成功，仍有大于 500 kB 的既有分块告警，性能验收未通过。
- Rust Team 定向日志：57 passed / 4 ignored，其中新增两项验证 idle barrier 等待文件同步锁和连接操作锁。clippy 与 no-default-features lib check 成功；这不是完整原生应用 / 全部 Rust 测试通过证据。
- 全新浏览器会话 `team-cancel-verified` 使用实际 App 与隔离 IPC 夹具，依次执行组织刷新、停止、延迟请求释放。停止期间导航、退出及采集按钮禁用；释放后恢复可操作并显示明确提示。记录到 `team_cancel → team_wait_idle → team_status`，没有自动重试预览或写入。新会话 errors=[]。
- 界面证据：`evidence/team-cancelling.png` 与 `evidence/team-cancelled.png`。夹具为合成组织，不能代替真实 Tauri、CLI 或 UAT。旧开发会话留有修改 hook 顺序时的 HMR 错误；重新加载后的独立会话未复现，不将旧错误抹去后冒称全程零错误。
- TeamAI 固定 SHA `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`，真实引入 frontmatter / rule-format 模块并校验上游字节哈希；worker 提供 inspect_skill、convert_rule、version，输出结构化结果。构建清单包含 7 个依赖的许可文件，9 项真实 worker 子进程测试通过。
- 当前 worker SHA256：`659c90f48a480b0be8408fff52f0493810056d3e78f8f5089a01c72bcdf8ec96`。此为开发产物，不是最终 Windows 包；全局 Node 缺失场景、Rust worker 超时与进程回收、服务端批准发布闭环仍未验收。
- 本轮 GitNexus status 索引 commit 与 HEAD 均为 fff87c3；detect-changes 显式指定 nexusops-client 后成功，18 个受跟踪文件 / 40 符号 / 25 流程，风险 CRITICAL。索引不完整覆盖未提交新文件及动态 IPC，因此结合源码、类型检查、Rust 与浏览器证据，不能以图查询代替实际验证。`git diff --check` 通过。
