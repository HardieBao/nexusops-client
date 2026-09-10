# UAT 工作区部署与回退验证

日期：2026-09-11（Asia/Shanghai）；远端时间使用 UTC。

## 当前版本

- 服务端提交 23a5657416db57938b78cf2c86a4533153a8220e 已快进推送到 origin/develop_hardie，无强制推送。
- UAT 工作区运行 `nexusops-teamai@sha256:4976a925a9907e5119502a9aa84727c34ed7b63a55f12cded70cfe325737414d`。
- `/health` 实测 200、status=ok、revision=23a56574…；`/health/workspace-entitlement` 实测 200、status=ready。
- Gateway 镜像包含同一源码版本构建的管理端。Platform、Website 和 CLIProxyAPI 启动时间均早于本次部署，未随工作区切换。

## 发布路径与备份

新官方 Deploy UAT run 34528466735、CI run 34528466125 已结束为 failure；gateway job 103043074225 的 annotation 再次确认账号付款/额度导致 job 未启动。没有把该状态改写为通过。UAT 环境 protection_rules 为空。

按 D11 使用同 SHA 本地完整检查与构建镜像，通过现有 SSH 身份、严格主机校验，执行仅 workspace 的受控直接部署。使用现有 `.deploy-uat.lock`，校验旧 revision、新镜像标签、项目和域名；不改其他服务镜像。

部署前备份工作区 PostgreSQL（custom dump、pg_restore --list 检查）、/app/data 资产目录（tar 列表检查）及 .env。备份仅保存在服务器私有制品目录，未把数据库或凭据下载到仓库/响应。

- 首次备份：`/home/nexusops-ci/teamai-artifacts/23a5657416db57938b78cf2c86a4533153a8220e/backup-20260910T205304Z-hKIUt2`
- 成功部署备份：`/home/nexusops-ci/teamai-artifacts/23a5657416db57938b78cf2c86a4533153a8220e/backup-20260910T205603Z-TmZSh1`
- 成功部署脚本 SHA256：9e8fb70b7f1e8b717ffe148ba26cf40bf6170cd8d2d943a18ef19a47c71e8e2b。

## 首次回退与原因

第一次新版本健康启动后，校验脚本错误地将迁移原文件 SHA256 与 schema_migrations 中的值比较，触发自动回退。旧版 9a3022a0 恢复后健康检查实际 200/ok；没有删除新增表或恢复覆盖业务数据库，证明旧应用在新增迁移保留时仍能启动。

源码 migrations_runner.go 使用 `SHA256(strings.TrimSpace(content))`，仓库 ledger 使用完整文件字节。核对源码与 Git blob 后，确认数据库值正确：

| 迁移 | 数据库规范化 checksum |
| --- | --- |
| 0265_teamai_command_receipts.sql | 6b1a52c4926fc8f8b79cd450d28d1afe75825f37400c0864350470cb67823f2b |
| 0266_asset_revision_source.sql | acbb31bf6c7ab596b158a7ef51b1fe7bdb2b586c53f2ad15bb7edc46bdeb8490 |

仅修正校验脚本后重新备份、部署，两条迁移和新版本健康检查均通过。没有修改迁移文件或数据库 checksum 来迎合检查。

## 部署后实际请求

GET projects、config、admin/assets，POST report、sync、ack 在无凭据时全部返回 401；管理端 `/admin/assets` 返回 200 页面资源。此项只证明路由和未认证边界，不证明登录后的权限与业务流。

## 剩余验收

尚需有效 UAT 测试管理员/成员身份完成候选审批发布、客户端真实下载安装、工具事件上传和管理员对应员工查询。已请求用户提供授权测试会话或凭据文件路径，未猜测密码、未重置凭据或绕过 MFA。

内置浏览器连接失败；独立测试浏览器也未能稳定保持登录页面，未将其视为已登录。客户端最新上报面板修复还需最终重打包及整体 UI/性能验收。T22 有部署与回退证据，但 T23/M5 尚未全部通过。
