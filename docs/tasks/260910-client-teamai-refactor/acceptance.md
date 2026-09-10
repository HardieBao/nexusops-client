# CLIENT-TEAMAI 验收进度

更新日期：2026-09-10。当前为实施中，不是最终完成报告。

| 任务 | 状态 | 当前证据 |
| --- | --- | --- |
| T00 | 完成 | baseline.md：两个仓库基线、工具版本、成功重建客户端索引、前端 1105 测试 / typecheck、Rust Team 55 通过 / 4 跳过、Gateway 4 包检查和实际 UAT health |
| T01 | 完成 | feature-mapping.md：15 类旧 View、9 类工具上下文、快捷键 / 原生入口 / 管理阻塞都有迁移目标；实际回归留给 T10 |
| T02 | 完成 | contracts.md + contract-examples.json：v1 边界、字段、方法、上限、scope / 项目映射、候选与 ACK、状态阈值、隐私与正反样例明确 |
| T03 | 实施中 | 新 shell、导航、标题 / 操作与页面出口均已拆出；1146 项全量前端测试通过。原生窗口 / 完整入口场景仍待后续验收 |
| T04 | 实施中 | Team 连接表单、组织信息、供应商与资产区块已拆出，多个页面复用同一 controller；跨身份取消 / 完整故障场景仍需补齐 |
| T05—T23 | 未验收 | 未缩小 task.md 的原目标；独立用量入口与首页只读摘要已先接入，但不代表对应完整任务已通过 |

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

TeamAI 模块尚未打包 / 执行；新 HTTP 协议未实现；Team 页面拆分的完整取消 / 身份切换验收尚未完成；工具真实状态、独立本机聚合、最终 Windows 包和新版本 UAT / 真实 CLI 联合验收仍未完成。

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
