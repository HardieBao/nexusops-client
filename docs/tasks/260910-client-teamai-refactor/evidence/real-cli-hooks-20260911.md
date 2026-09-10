# 真实 CLI → 发布版采集器检查点

2026-09-11。T16 / T23 未通过：Claude 链路已取得实际证据，Codex 链路失败。

新增显式运行的 `services::team::tool_usage::cli_tests`，用真实 CLI、实际 release/nexusops-client.exe、独立配置/队列/工作目录及本地合成模型响应。没有个人模型凭据、个人会话或真实模型费用。测试配置由生产 hooks::merge 生成，SQLite 使用生产 UsageStore；默认 ignored 要求显式 CLI/采集器路径，不将跳过当作通过。

结果：真实 Claude Code 触发 3 个生命周期事件，release 采集器写入实际队列；日聚合总数一致，ACK 清队列后历史不减。原始与带诊断的重复运行均通过。

Codex 0.153.2 未收到采集事件，保持失败。已排除/确认的事实：

- features list 显示 hooks stable=true。
- 官方文档要求非 managed Hook 按哈希审阅信任；测试对完全自建的隔离 Hook 使用官方一次性 trust-bypass 选项，没有修改用户持久信任或产品流程。该选项并未解决本次失败。
- 移除 ephemeral、移出系统 TEMP（消除辅助程序路径告警）、将模型失败响应改为成功 SSE，均未使采集通过；成功 SSE 下 CLI exit=0，但事件仍为 0。
- 实际 app-server hooks/list 能识别生产格式的四项 Hook，enabled=true、untrusted，warnings/errors 为空。
- 最小 echo Hook 实际写出了标记；探针确认运行 shell 为 Windows PowerShell Desktop。
- 对采集命令临时试验 start /wait、PowerShell 调用运算符、显式管道，均未通过。已移除这些测试覆盖，保留生产命令的失败复现，没有把未验证假设写入生产 hooks.rs。

参考：[官方 Hooks 文档](https://learn.chatgpt.com/docs/hooks)。另读取 rust-v0.153.2 上游 exec 测试、hooks discovery/command_runner 与 core/session 源码辅助定位；源码推断不能代替真实采集断言。

当前日志在执行机器 TEMP 下：nexusops-real-cli-hooks.log、nexusops-real-cli-hooks-verified.log、nexusops-codex-hooks-success-response.log、nexusops-codex-hooks-pipeline.log 等。诊断仅输出 Hook/错误摘要，未将真实用户输入保存为证据。

继续方向：在完全合成 Hook 中记录是否进入命令、收到的字节数和事件名，区分 shell 调用、stdin 转交、collector 参数/数据库路径三个环节。先使真实 Codex 失败用例转绿，再决定生产修改、完整回归与重建安装包。当前安装包的 Codex Hook 端到端能力未经通过验证，不能宣称核心全部完成。

## 后续：已验证原因与修复

修正前面的当前结论，保留失败过程作为历史：

- 最小环境漏掉 PATHEXT。直接比较 PowerShell 执行 Python --version：无 PATHEXT 返回 0 但没有输出；设置 `.COM;.EXE;.BAT;.CMD` 后正常执行。此前 GUI 子系统、等待、stdin 的实验受此测试环境缺陷影响，不能作为产品根因。
- 在正确环境中，原来只有引号路径的 Windows 命令在 PowerShell 返回解析错误；加调用运算符后，实际 release 采集器收到事件。生产 hooks.rs 的 Codex commandWindows 已改为 `&` 调用，并使用 PowerShell 单引号字面量保护路径，单引号按双单引号转义。Claude 命令未改。
- 删除测试中的临时命令覆盖，真实测试使用生产 merge 输出。Codex 和 Claude 各 3 个事件到达实际 release 采集器，去重/日聚合数一致，ACK 后历史保留；Codex 队列路径包含 `$` 和单引号。
- `nexusops-real-cli-hooks-fixed.log`：2 passed / 0 failed / 0 ignored；完整 Rust `nexusops-hooks-fix-full.log`：2874 passed / 0 failed / 19 ignored；clippy 通过。GitNexus merge upstream 为 LOW，修改后 detect_changes 已运行，累计工作树仍为 CRITICAL。
- 真实测试仍对自建 Hook 使用一次性信任选项；产品 `/hooks` 人工确认以及 UAT 没有被这项测试代替。其他自定义 Windows shell 未在此测试覆盖。

开始重新生成 Windows NSIS 包，日志 TEMP/nexusops-teamai-windows-hooks-fixed-package.log。先前 SHA256=60ff5455… 的安装包不包含新命令生成修复，不能作为修复版交付。

清理限制：诊断过程中创建的 `.teamai-build/collector-console-probe.exe` 有进程 4048、45332、49232（无窗口）。自动审批拒绝了停止/删除诊断副本的清理命令，仅返回 blocked by policy；已停止清理尝试。该诊断 EXE 不在 bundle.resources 清单内，不会被打入安装包。需要后续获准清理；不将此项隐去或误称测试环境已全部清理。
