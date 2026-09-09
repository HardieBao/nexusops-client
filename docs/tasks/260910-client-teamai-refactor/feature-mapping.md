# 旧功能与新入口迁移矩阵

基线：客户端 `f811e352`，依据 `src/App.tsx`、`src/config/appConfig.tsx`、设置页与已有集成测试。下表固定功能归属；尚未代表这些入口已实现或已通过回归。

## 15 类旧 View

| 旧保存值 | 现有组件 / 行为 | 新入口 | 保留方式与测试重点 |
| --- | --- | --- | --- |
| providers | ProviderList、AppSwitcher、Provider 对话框、代理接管 | 工具接入 → 当前工具 / 个人配置 | 保留所有供应商 CRUD、切换、复制、usage script、工具专属默认行为；仍用原处理函数 |
| settings | SettingsPage，多标签与导入导出 | 设置 | 旧页继续打开；原 usage 标签兼容跳转或复用独立使用记录；Ctrl/Cmd+, 仍打开设置 |
| prompts | PromptPanel、内容编辑、主动作切换、离页阻塞 | 资产库 → 个人 → Prompts | 保存值直接打开原编辑器，不在有未确认编辑时强制切页 |
| skills | UnifiedSkillsPanel、更新检测、安装与来源 | 资产库 → 个人 → Skills | 保留导入 / 更新 / 安装，导航阻塞有效 |
| skillsDiscovery | SkillsPage、来源选择 | 资产库 → 个人 → 发现 Skills | 返回先到 skills；初始工具映射与当前实现一致 |
| mcp | UnifiedMcpPanel | 工具接入 → 高级 → MCP | 原管理状态阻止离页；Pi 不支持时保留原回退行为 |
| agents | AgentsPanel | 资产库 → 个人 → Agents | 直接入口保留，不能被新的团队来源过滤掉 |
| universal | UniversalProviderPanel | 工具接入 → 高级 → 通用供应商 | 保留独立管理入口，不强迫映射成团队 Key |
| sessions | SessionManagerPage | 使用记录 → 本机会话 / 高级入口 | 按当前工具挂载，切换工具更新列表；无会话能力时回退 |
| workspace | WorkspaceFilesPanel | 工具接入 → OpenClaw → 工作区文件 | 不是新的首页，不把旧值 workspace 映射成工作台 |
| openclawEnv | EnvPanel | 工具接入 → OpenClaw → 环境配置 | 只在对应工具上下文提供入口，保留已有配置行为 |
| openclawTools | ToolsPanel | 工具接入 → OpenClaw → 工具配置 | 与 MCP / 组织远程命令分开 |
| openclawAgents | AgentsDefaultsPanel | 工具接入 → OpenClaw → Agent 默认配置 | 不与个人 Agents 注册页合并数据 |
| hermesMemory | HermesMemoryPanel | 工具接入 → Hermes → Memory | 保留 Hermes WebUI 启动确认与相关入口 |
| team | TeamPage，连接、provider、资产、统计 | 组织与上报（旧入口可保留全部任务的定位入口） | Team 编译功能关闭时不调用其 IPC；团队工具 / 资产拆页后同一上下文 |

## 新的稳定入口

| 主导航 | View / 页面契约 | 允许的页面副作用 |
| --- | --- | --- |
| 工作台 | `home`，真实上下文摘要及未连接引导 | 只读本机状态；网络刷新明确触发或遵守既有读取策略 |
| 工具接入 | `providers` 为现有个人工具主视图，提供团队接入入口 | 进入本身不替换当前 Provider；保留原动态读取 |
| 资产库 | `assetLibrary`，团队与个人来源切换 | 只读清单；确认后才调用写入 / 同步 |
| 使用记录 | `usage`，复用 UsageDashboard 与保存偏好 | 按已保存策略读取 / 刷新，不自动重建历史 |
| 组织与上报 | `team` / 拆分后的组织页 | 进入不自动连接、注册 Hook 或启用统计 |
| 设置 | `settings` | 保留原显式保存和已有明确的偏好自动保存行为 |

旧 view 保存值全部继续识别；新增 view 使用同一保存 key。首次没有保存值时进入 home；未知保存值回到 home。不启用 Team 的构建仍能使用本地工作台、供应商、资产和用量；旧的 team 值保持安全回退到 providers。

## 工具能力边界

- `APP_IDS` 当前包含 claude、claude-desktop、codex、gemini、grokbuild、opencode、openclaw、hermes、pi；设置隐藏的工具按当前可见列表回退。
- Claude Desktop 的共享配置上下文继续映射到 claude；其特殊 route toggle 保留。
- 完整 proxy / failover 现有列表为 claude、codex、gemini、grokbuild；OpenClaw / Hermes / Pi 的额外行为不能由「代理未启用」替代。
- Pi 无 MCP 注册能力；OpenClaw 保留其工作区 / env / tools / agents 专属配置入口。
- Skills 页面目前对 OpenClaw 使用已有回退上下文；此次导航重排不把它宣称为已新增原生 Skills 支持。
- TeamAI 新融合首发只承诺 Codex / Claude Code；其他工具现有个人功能不会因此删除。

## 跨页面入口与状态

| 入口 / 状态 | 迁移要求 |
| --- | --- |
| Ctrl/Cmd+, | 保留设置快捷键；已有 managementBusy 时阻止切换 |
| Escape | 文本编辑与模态场景继续不抢按键；高级页回到其业务入口，skillsDiscovery 先回 skills |
| 顶部返回、工具图标、设置 usage 快捷入口 | 使用统一导航函数；不能绕过待确认编辑与敏感操作阻塞 |
| provider-switched、导入成功、后台同步事件 | 保留监听与缓存失效；不因新侧栏重复注册 |
| 原生窗口与托盘 | 标题栏拖动区域不能遮挡导航；原按钮处理函数与托盘事件保留 |
| 深链与系统文件选择器 | 继续走原解析 / 导入确认流程，不把 URL / Profile 内容当作已授权操作 |
| Team feature flag | 状态未加载时不提前挂载 Team 命令；显式关闭时隐藏团队操作并保留本地工作流 |
| 切换工具 / 组织 | 旧异步结果失效；可恢复的本机记录与远端授权状态分别保留 |

本矩阵将由 T10 对每一行补充实际测试结果；本次源代码核对完成不等于原生回归已完成。
