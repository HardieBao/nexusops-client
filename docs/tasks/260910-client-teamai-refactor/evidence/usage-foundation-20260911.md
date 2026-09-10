# T17 开始实施：队列计数与身份前置检查

2026-09-11；工作树未提交，未完成 T17。

当前发现：`connection_id` 由网关 URL、organization_id、workspace_id 计算，没有员工字段。因此它只能标识组织连接，不能作为本人日聚合的唯一分区键。后续聚合必须使用服务端 `teamai_project` 已校验的 member_id 加组织/工作区作用域；不得把本次不同 connection_id 的测试称为同组织不同员工隔离已通过。

本轮修改 `tool_usage.rs`：

- 对不匹配的 connection_id，status 不返回其他连接的 pending、uploaded、last_uploaded_at 或 last_error。
- acknowledge 只将实际删除的事件数计入 uploaded。重复 ACK 不再增加计数，也不刷新最后上传时间。
- 不增加任何遥测字段，不改变 Hook 事件映射；没有将 uploaded 冒充 30 天历史。

本轮实际验证：MSVC 2019 下 `cargo test --manifest-path src-tauri/Cargo.toml --lib services::team::tool_usage`，6 passed、0 failed、0 ignored；覆盖 20 个唯一事件、前 5 个先确认、整批重试、重复确认与重启计数。`cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` 和 diff 检查通过。

GitNexus：修改前 status / acknowledge / capture / open 的 upstream 分析为 LOW，直接调用覆盖配置、上传和测试，动态 Hook 仍需结合源码。修改后 detect_changes 为累积工作树 35 文件、171 符号、36 流程、CRITICAL；不是本轮两个函数单独的风险等级，也不代表所有累积修改已完成验收。

剩余：服务端确认员工身份的本机绑定；独立去重键和 UTC 日聚合；30 天清理；退出与重新连接行为；只读 API、清理动作和 UI；真实 Hook 与 UAT 统计对账。T17 / S27 未通过。
