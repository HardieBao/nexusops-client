# CLIENT-TEAMAI 验收进度

更新日期：2026-09-10。当前为实施中，不是最终完成报告。

| 任务 | 状态 | 当前证据 |
| --- | --- | --- |
| T00 | 完成 | baseline.md：两个仓库基线、工具版本、成功重建客户端索引、前端 1105 测试 / typecheck、Rust Team 55 通过 / 4 跳过、Gateway 4 包检查和实际 UAT health |
| T01 | 完成 | feature-mapping.md：15 类旧 View、9 类工具上下文、快捷键 / 原生入口 / 管理阻塞都有迁移目标；实际回归留给 T10 |
| T02 | 完成 | contracts.md + contract-examples.json：v1 边界、字段、方法、上限、scope / 项目映射、候选与 ACK、状态阈值、隐私与正反样例明确 |
| T03 | 实施中 | 新 shell、导航定义、六个稳定入口、旧 view 恢复、页面返回映射已接入；本轮 39 项定向测试与 typecheck 通过。页面标题 / 操作进一步解耦及原生验证待完成 |
| T04—T23 | 未验收 | 未缩小 task.md 的原目标；独立用量入口与首页只读摘要已先接入，但不代表对应完整任务已通过 |

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

TeamAI 模块尚未打包 / 执行；新 HTTP 协议未实现；Team 页面仍待拆分；工具真实状态、独立本机聚合、最终 Windows 包和新版本 UAT / 真实 CLI 联合验收仍未完成。

已读到 UAT 基线版本 9a3022a0，不等于本次重构已部署。最终必须逐项满足 task.md 的 O1—O6、M1—M5 和 S01—S30。
