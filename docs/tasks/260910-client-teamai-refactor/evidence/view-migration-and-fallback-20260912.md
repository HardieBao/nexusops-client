# 保存页面迁移与隐藏工具回退

2026-09-12，T03 / T10 / S02 局部验收。原生观察基于 74eddefe 的 release EXE；本文对应的回退修复在该基线上继续修改 src/App.tsx，不包含在 74eddefe 安装包中。

## 实际 EXE 重启恢复

在独立应用数据、工具配置和 WebView 目录下，写入旧版本的 last-view / last-app 值；每次正常退出进程后重新启动 EXE，读取页面标题、主要内容、操作按钮及四个主导航。没有替换 Tauri IPC，也没有以浏览器 reload 代替进程重启。

逐一打开 providers、settings、prompts、skills、skillsDiscovery、mcp、agents、universal、sessions、workspace、openclawEnv、openclawTools、openclawAgents、hermesMemory、team；专属页使用相应工具。附加未知保存值、无保存值、Pi 的旧 MCP 页面，共 18 个场景。

初次标题断言 15/18 通过，三个失败为探针期望错误：Prompts 带工具名称前缀，skillsDiscovery 使用原 Skills 标题，MCP 使用统一面板标题。核对实际 ClientHeader 的明确标题分支后，以正确期望重新启动这三个场景，3/3 通过。初次失败记录保留在 `native-view-migration-74eddefe.json`，复测为 `native-view-title-retest-74eddefe.json`，没有修改生产标题迎合测试。

这些证据证明 15 类页面可恢复、主要内容已挂载及导航存在；不证明全部编辑、保存、离页阻塞、工具高级操作均通过。无数据夹具也不能代替真实用户配置迁移。

## 新发现的真实缺陷与修复

补测把 Hermes 设为隐藏，保存 hermesMemory / hermes 后重启。活动工具已回退，但页面仍是 Hermes 记忆管理，未与当前工具一致；实际结果见 `native-hidden-view-before-74eddefe.json`。

修复在既有 App 页面能力回退副作用中增加五类专属页面判断：Hermes Memory 只保留在 Hermes 上，OpenClaw 工作区 / 环境 / 工具 / Agents 页只保留在 OpenClaw 上。不匹配时回到工具接入；合法工具上的专属页面继续保留。没有修改资产写入、网络认证、工具配置或权限合同。

先添加回归测试：5 个不匹配页面与 1 个隐藏 Hermes 场景均失败；5 个合法专属页面保留场景通过。修复后 11 个新增场景通过。首次 typecheck 发现测试夹具只给出部分 VisibleApps，已使用完整默认配置再隐藏 Hermes；生产修复未因此改变。

前端完整检查为 147 文件 / 1192 tests 通过；typecheck 和 renderer build 通过。构建仍有既有大分块告警。Rust / 服务端未修改，本轮不重报历史 Rust 测试为新的执行结果。

## GitNexus 与证据限制

分支 codex/client-teamai-refactor，索引原指向 4d33d55，源码 HEAD 为 74eddefe。按 `gitnexus analyze --index-only` 刷新时失败：FTS `file_fts` document offset 1437 missing during delete。随后 App upstream impact / context 均返回符号不存在，risk UNKNOWN；detect-changes 返回 No changes detected，但实际 Git diff 有 App 和测试修改，因此该结果不能覆盖本轮变化。

源码确认入口为 src/main.tsx → App，页面恢复、可见工具回退与工具能力副作用均位于 App。使用真实 EXE 失败复现、相关集成测试、全量前端检查和实际 diff 补足本轮验证。没有将零图结果称为低风险。

剩余：从包含修复的新 EXE 再执行隐藏工具场景与合法页面保留；完整旧功能交互、托盘及 UAT 已认证联调仍未完成。
