# CLIENT-TEAMAI 实施基线

记录日期：2026-09-10。执行者负责本机自动化验证；真实工具和 UAT 联合验收按 T23 单独记录操作人与证据，不把本文件当作最终验收。

## 工作范围

- 客户端：`D:/Hardie作品/nexusops-client`，起点 `f811e352fe3904214a2b2305baff81964a416c56`；实施分支 `codex/client-teamai-refactor`。
- 主项目：`D:/Hardie作品/nexusops/.worktrees/team-ai-control-plane`，`9a3022a02dbe1cbbe6d6823e04d18c14a9b831e6`；沿用其已有隔离工作树。
- 客户端开始时仅 `docs/design/`、`docs/tasks/` 未跟踪，都是本次设计 / 计划文件；无未提交产品代码。主项目实施工作树干净。原客户端 Team 分支保留在起点，新功能在新分支上实现。
- 此次没有更改 Git 远程配置、现有运行时 Hook、真实凭据或员工文件。

## 实际工具链

| 工具 | 本轮读取结果 |
| --- | --- |
| Windows | NT 10.0.26200.0，Windows x64 |
| Node | v24.18.0（开发工具；交付包运行时另由 T11 固定） |
| 客户端 pnpm | 10.12.3 |
| 主项目 pnpm | 清单要求 11.7.0，按所属项目运行 |
| Rust / Cargo | rustc 1.95.0 / cargo 1.95.0 |
| Go | go1.25.4 windows/amd64 |
| Codex | codex-cli 0.153.2 |
| Claude Code | 2.1.263 |
| MSVC | 既有 VS 2019 BuildTools `vcvars64.bat`；原生 Rust 测试使用该环境 |
| 本机 SQL 夹具 | PostgreSQL 18.3，PGlite 0.5.8；在临时工具目录中运行，不进入产品依赖 |

## UAT 当前事实

本轮实际请求 `https://uat-api.ai-nexusops.app/health` 成功，响应如下字段：

```json
{"status":"ok","service":"gateway-server","revision":"9a3022a02dbe1cbbe6d6823e04d18c14a9b831e6","version":"uat-9a3022a02dbe1cbbe6d6823e04d18c14a9b831e6","build_time":"2026-09-09T16:08:26Z"}
```

因此旧对话中的「9a3022a0 尚未部署」已过时。该检查只证明 Gateway 当前版本与可达性，不证明本任务新功能、数据库迁移或控制台版本已验收。UAT PostgreSQL 的实际版本、数据库备份和最终各组件版本将在部署前读取；本轮没有导出真实服务器配置。

## 基线检查

- 客户端 `pnpm typecheck`：通过。
- 客户端 `pnpm test:unit`：138 个文件 / 1105 项测试通过，退出码 0。
- 主项目 `go -C apps/gateway-server test ./internal/toolusage ./internal/handler ./internal/server/middleware ./internal/server/routes`：四个包通过（Go 测试缓存命中已如实保留）。
- 客户端原生 Team Rust 基线：55 项通过、0 失败、4 项既有外部环境依赖跳过；测试二进制真实执行，退出码 0。跳过的是 Windows 凭据写入及三个需要隔离真实 Gateway 夹具的测试，不把这些标为实机通过。

## GitNexus

客户端起初索引指向旧提交且有未完成增量标记。执行 `gitnexus analyze --index-only --workers 4` 后自动完整重建成功：24,607 nodes / 56,317 edges / 300 flows，77.5 秒；没有注入或覆盖 AGENTS / skills。

`App` upstream impact 返回 LOW / 0 上游符号；源码明确显示它由 `src/main.tsx` 渲染，不能把图中的 0 当作没有调用者。后续导航变化使用真实 App 交互测试补足 React / JSX 图覆盖。

主项目已有工作树索引问题不作为客户端开发阻塞；真正修改 Gateway 前重新检查当前状态，失败则记录当轮原因并核对源码与聚焦测试。
