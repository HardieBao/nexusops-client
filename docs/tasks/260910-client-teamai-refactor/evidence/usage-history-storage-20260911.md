# T17 本机日聚合实现检查点

2026-09-11，未提交工作树，T17 未完成。

实现：

- 在现有 tool-usage.db 增加独立历史绑定、事件去重和日聚合表；只有 runtime、event、UTC 日、计数、最后观察时间和事件哈希 ID 入库，不保存 Hook 原文。
- 启用采集时通过认证的 TeamAI projects 接口取得并校验 member_id，使用连接作用域与 member_id 的哈希绑定历史。原队列不能反推出本人历史，不做虚构回填。
- 采集事件的队列写入、去重和聚合在同一事务；ACK 只删除队列。保留今天及之前 29 个 UTC 日期，清理去重和聚合记录。
- 重新接入或 Key ID 改变时停用采集、清队列并解除历史绑定；旧聚合留到到期或本人清理。再次绑定同一员工可以读取本人保留记录，但不会自动启用采集。
- 新增 team_tool_usage_history 与 team_clear_tool_usage_history IPC。读取与清理经过服务操作锁，与连接切换串行。清理仅影响当前绑定历史，不删除待上传事件或服务端统计。

存储测试覆盖：20 个事件 ACK 后历史保留、禁用不新增、同工作区成员切换隔离、重新绑定与重启读取、重复 ID、UTC 时区归一、30 天边界、历史清理与 outbox 相互独立。

风险与限制：连接入口 GitNexus upstream 为 CRITICAL（图含同名符号关联，结合源码判断），本轮修改前已说明；采集/status/ACK 分析为 LOW。累积 detect_changes 为 CRITICAL，不能用于宣称整个工作树已完成验收。初次 clippy 因历史读取未接线失败，已增加真实 IPC 调用，没有压制 dead_code。

仍需：前端历史 API、使用记录与工作台展示、清理确认、Hook 注册与最近观察的独立状态、旧服务端版本提示、真实工具及 UAT 数据对账。最终检查结果另见下方，不以先前测试结果替代最终代码验证。

最终代码在 MSVC 2019 环境执行完整 `cargo test --lib`、`cargo clippy --lib -- -D warnings`、`cargo check --no-default-features --lib`，整组进程退出码 0。日志为本机临时目录 `teamai-history-verified.log`、`teamai-history-verified-clippy.log`、`teamai-history-verified-disabled.log`。默认忽略的环境依赖测试未被此轮自动计作通过；此轮没有 UI 或 UAT 验收。
