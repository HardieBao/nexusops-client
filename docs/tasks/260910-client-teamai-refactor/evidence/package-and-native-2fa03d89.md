# 2fa03d89 Windows 包与原生回归

2026-09-12。客户端源码 `2fa03d89229c1d2dbb3676a0bdf590567f650659` 已推送至 develop_hardie；Gateway / 管理端配对 `23a5657416db57938b78cf2c86a4533153a8220e`。本记录是局部验收，不代表 T00—T23 / S01—S30 全部通过。

## 构建与文件校验

使用 VS 2019 vcvars64 与 `pnpm build:teamai:windows --bundles nsis --no-sign --config <临时关闭 updater artifacts 的配置>`。实际构建 exit 0，Rust release 6m51s，NSIS 成功；构建期间只有文档修改，程序源码与 2fa03d89 一致。

- 安装包：`src-tauri/target/deliverables/2fa03d89/NexusOps-Client-0.1.0-2fa03d89-x64-setup.exe`。
- 包 SHA256：`7e19997e6ef80da410b45b18f44a15f743ee7d1ed38c11cc11243a00262952d3`。
- 构建 EXE SHA256：`fa0c48bb4d2ac8c006d38036cdd1dd9d35a000661e8cf6d93ef7dab79d1f0e38`。
- 安装 EXE SHA256：`e49363c4b4e176a8fb4fb4ef1d99e0699f9c6e3314d177982488bef1646c9e1a`。
- 12 个 TeamAI 运行资源与上一候选的相同固定清单逐项哈希一致。EXE 仅有 Tauri UNK → NSS 打包标记三字节差异，其余字节与构建产物一致。完整清单见 `package-2fa03d89.json`。

## 实际安装后的回归

先确认没有既有产品卸载记录或活动客户端，备份本产品协议、安装位置、同名快捷方式与开机启动项。NSIS 实际安装在独立中文和空格目录，exit 0。

使用安装目录 EXE、独立应用 / Codex / Claude / Hermes / WebView 目录，PATH 只有 Windows System32。每个场景先保存偏好和旧页面值，正常退出后启动新进程；未使用模拟 IPC。

| 场景 | 结果 |
| --- | --- |
| 隐藏 Hermes 后恢复 hermesMemory / hermes | 回到工具接入，通过 |
| 在 Claude 上恢复 OpenClaw 的 workspace / env / tools / agents 四类页 | 全部回到工具接入，通过 |
| 在 OpenClaw 上恢复其四类专属页 | 全部保留，通过 |
| 在可见 Hermes 上恢复记忆页 | 保留，通过 |

合计 10/10，数据见 `native-fallback-2fa03d89.json`。这补足原生缺陷的修复后验证；此前 74eddefe 的实际失败仍保留，不改写为通过。

另从安装目录调用真实 teamai_run/version：返回固定 0.22.0 / 6ae0619d… 及三种受支持操作。使用 Windows 原生键盘输入：Ctrl+, 实际进入设置，Escape 返回工作台；Tab 聚焦工作台 / 工具接入时 :focus-visible=true、轮廓为 solid，Enter 进入工具接入。截图 `installed-2fa03d89-workbench.png` 为隔离数据的实际安装程序。

## 卸载与环境恢复

正常退出后实际卸载 `/S _?=<测试目录>`，exit 0。程序和卸载记录移除，测试数据库保留。按上次已核对的 NSIS 语义，清理本次专有的安装位置键与卸载器残留；路径、键值和目录内容先做边界检查。协议导出哈希、快捷方式与开机启动项恢复为测试前状态。结果见 `installed-2fa03d89-check.json`。

## 环境复核与限制

- 本轮 UAT /health 再次返回 revision=23a56574、status=ok；workspace-entitlement=ready。尚未获得授权测试身份，未执行登录后的审批、资产安装与管理员统计对账。
- GitNexus 定向 `--repair-fts --index-only` 也失败，报 Failed calling LOWER: Invalid UTF-8；没有冒称已修好图索引。此前源码回退修复已有真实失败 / 修复后用例、1192 项前端测试、typecheck 和 renderer build 证据。
- 当前安装验证属于现有 Windows 用户下的隔离目录，不等于全新 Windows 用户或 VM；托盘菜单、全部旧功能编辑流程和完整 UAT 场景仍待验收。
