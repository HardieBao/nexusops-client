# UAT 版本与托管部署检查

2026-09-11，本轮实际读取，未触发部署。

- Gateway `/health` 返回 status=ok，revision=`9a3022a02dbe1cbbe6d6823e04d18c14a9b831e6`，version=`uat-9a3022a02dbe1cbbe6d6823e04d18c14a9b831e6`，build_time=`2026-09-09T16:08:26Z`。本任务新增 TeamAI/来源迁移尚未部署。
- GitHub Deploy UAT run `34373712360` 已结束，conclusion=failure；quality-gate jobs 没有执行 steps，preflight/build/deploy 被跳过。
- 实读 gateway-server job `102541057654` 对应 check annotations：因账号近期付款失败或 spending limit 需要提高，job 未启动。此为托管运行条件失败，不是本任务代码检查结果，也不能宣称托管 CI 通过。
- CI run `34373711673` 同样失败；Security Scan run `34373711870` 失败。本记录没有修改账单、账户设置、审批或工作流。
- 现有部署入口 `.github/workflows/uat-deploy.yml` 仅允许 develop_hardie，依赖质量检查与 uat 环境；`infra/vps/deploy-uat.sh` 要求 immutable GHCR digest 和完整 40 位 SHA，包含环境及备份前置条件。

后续需完成：当前源码验证/版本固定、产物摘要、迁移与备份、可执行的受控部署路径以及实际 UAT 联调。D11 允许准备现有受控直接部署方案，但不允许把未通过的托管检查写成通过或跳过环境审批。

Windows NSIS 构建 session 81018 仍活跃，rustc CPU 持续增长，无报错；保持复用该进程。上一轮已验证构建输入资源哈希和许可证，但安装包尚待产出与核验。
