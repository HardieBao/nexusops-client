# 实际 Rust → Go 候选与发布联调

日期：2026-09-11。所附两个 JSON 为合成测试资源，经实际 Rust `export` 和固定 TeamAI worker 生成，不含员工文件或凭据。并非手工拼出的 HTTP mock。

运行步骤：

1. 客户端 `pnpm teamai:prepare`，设置 `NEXUSOPS_CANDIDATE_FIXTURE_DIR` 为新建的空绝对目录。
2. 在 MSVC 2019 环境运行 `cargo test --manifest-path src-tauri/Cargo.toml --lib export_cross_language_candidates -- --ignored`。实际结果：1 passed，0 failed，0 ignored；两份候选均成功输出。重复运行需新目录，导出器拒绝覆盖。
3. Gateway 测试进程使用同一 fixture 目录，并设置 `NEXUSOPS_TEST_DATABASE_URL` 指向专用合成数据 PostgreSQL；运行 `go test ./internal/adapter/persistence -run TestRustCandidatePublicationIntegration -count=1 -v`。实际 Skill / Rule 两个子测试均 PASS。

实际验证：

- 上游模块 commit 为 `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`，包含 CRLF 输入、中文文件名及二进制文件。
- Go 上传服务验证两种候选后创建真实 PostgreSQL revision，哈希与 Rust 导出值一致；来源正确保存；重复导入复用同一 revision。
- 成员 Key 上传被拒绝；Stable 发布前成员 manifest 为空、下载拒绝。
- 直接 Stable 发布缺少 Canary 时拒绝；Canary 后缺少审批仍拒绝；审批后成功发布。
- 发布后成员 manifest 返回正确 revision 和 hash，下载 Rule 字节相同，Skill 规范化后的文件清单与哈希相同。

边界：只有 blob 对象存储使用进程内 map；服务、权限、规范化、发布和 PostgreSQL 持久化使用生产代码。此测试直接调用服务层，不代表实际 HTTP 服务器、Rust 安装器、真实 CLI 或 UAT 联调。HTTP multipart 权限另有测试，本记录不拼接这些证据后声称端到端通过。

本轮还通过：`cargo fmt --check`、Go persistence 包 vet、两仓库 diff 检查。客户端 GitNexus detect_changes 执行成功，但新文件及动态调用覆盖有限；Gateway 工作树依旧未注册，detect_changes 返回 Repository not found。T13 / M3 仍未整体验收。
