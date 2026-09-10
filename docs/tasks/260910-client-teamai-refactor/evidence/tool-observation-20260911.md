# Hook 注册与实际观察状态

2026-09-11，未提交工作树；T16/T17 尚未整体验收。

原生新增只读检查：读取 Codex hooks.json / Claude settings.json（遵循既有配置目录选择），最多读取 1 MiB；逐项核对本客户端四类 Hook、命令、目标目录和 matcher。结果区分 installed、not_installed、needs_repair、unreadable、disabled_by_tool。检测不会修改文件或执行配置里的命令。

观察来自当前本人分区的实际 native event 聚合，与注册和采集开关分开。超过 24 小时为 stale，恰好 24 小时仍为 observed；无保留事件为 unobserved；未来时间不作为观察证据。新 IPC team_tool_observation 使用服务操作锁，返回两种工具的独立状态。

使用记录页读取并展示这些状态，支持已注册但采集关闭、注册需修复但采集开启等组合；四种界面语言已补齐。不推断 CLI 版本支持，也不把旧观察解释为工具故障。

验证：

- 原生 tool_usage 11 项测试通过；新增检查覆盖有效配置、旧 EXE 路径、重复、缺失、disableAllHooks、非法/超限文件及读取前后字节不变；观察测试覆盖工具隔离、24 小时边界、未来时间。
- clippy（-D warnings）和 no-default-features lib check 通过；本轮没有重复运行完整 Rust 测试，不沿用上一轮全量通过作为本轮全量证据。
- 前端 typecheck、147 文件/1178 项全量测试、renderer build 通过；最终仅调整状态段落位置与补充断言，相关 3 项组件测试再次通过。
- 浏览器 observation-ui 使用实际 App 与合成 IPC 响应，验证独立状态显示，errors 为空；截图 tool-observation-ui.png 中事件与状态均为夹具，不是本机员工记录或 UAT 证据。
- GitNexus 查询已有 merge/install 链路；新模块未完整入图。逻辑修改后 detect_changes 成功，累积风险仍为 CRITICAL；diff 检查通过。

剩余：工作台摘要和恢复动作入口；两种真实 CLI/Windows 交付包的 Hook 注册及事件观察；旧服务端兼容提示；UAT 数据对账。尚不能将“配置已注册”当作真实工具 Hook 生效验收。
