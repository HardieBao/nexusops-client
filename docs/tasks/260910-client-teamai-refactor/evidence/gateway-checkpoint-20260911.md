# Gateway / 管理端检查点

2026-09-11，本地提交 `23a5657416db57938b78cf2c86a4533153a8220e`，分支 codex/team-ai-control-plane；未推送、未部署。提交后工作树干净，主工作树的其他改动未包含在此提交中。

提交范围 44 文件：候选导入和来源、TeamAI 项目/config/report/sync/ack、对应权限与审计、管理端候选预览、OpenAPI、0265/0266 迁移及测试。未改变 Platform/Website 业务。

本轮实际检查：

- Gateway `go test ./...`、`go build -mod=readonly ./cmd/server`、`go vet ./...` 通过；测试连接专用 PostgreSQL 容器的合成数据库。
- 管理端 192 文件/837 项完整测试、typecheck、lint:check、build 通过。
- assets:check 与迁移 checksum/ledger/head 检查通过，workspace 迁移头为 0266。
- golangci-lint 2.13.1 首次报告两处测试问题。响应体关闭改为检查错误；跨语言夹具改用 os.OpenRoot 限定目录读取。对应 routes/persistence 测试（含真实 Rust 导出候选）重跑通过，lint 最终 0 issues。
- 暂存 diff、范围及 diff --check 已核对。GitNexus staged detect_changes 仍因该工作树未注册而失败；没有将其他工作树结果作为本提交图证据。此覆盖缺口已在提交前说明。

UAT 只读检查：环境 protection_rules 为空；利用现有 nexusops-ci SSH 隧道目标和已有私钥、严格 known-host 校验成功读取容器信息。现有项目目录为 /home/nexusops-ci/nexusops-uat/infra/vps，workspace 容器仍运行 git-9a3022a0，镜像 ID 为 sha256:003fe32a9bdd93b084ec4e3dc6e06f2d260ada7eb6d916d6bbd5010d121992b4。没有修改远端部署或读取密钥内容。托管 Actions 仍有付款/额度运行条件问题，后续需按 D11 完成受控部署准备与实际回退验证。

客户端修复版 NSIS 构建已成功结束，当前安装包 SHA256 为 b6524f50c6e72ce444afbb1c43dc74b7d1b6641fc0bd55bee1e33222d92ca59a；release EXE 为 7f5520dc27407021859456100fc220161143cd6b5cf08fea5342e7eb6d0fceb3。新包尚待原生复验，不能沿用旧包哈希或安装结果。

诊断副本清理：自动审批曾拒绝停止/删除操作，现已向用户请求对准确进程/路径的授权；尚未收到回复，未重试该操作。此项与后续 UAT/原生验收仍保持待完成。
