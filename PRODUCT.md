# NexusOps Client

<!-- impeccable:product-schema 1 -->

## Platform

web

React 界面运行在 Tauri 原生桌面窗口；保留 Windows / macOS / Linux 的窗口、托盘与文件选择行为。本次新融合的验收首发为 Windows x64。

## Users

团队员工是工作台的主要用户，同时保留个人开发者的本机配置能力。管理员在 NexusOps 网页后台管理成员、权限、资产审批、额度与组织统计。

## Product Purpose

通过一个桌面工作台连接组织、接入 AI 工具、预览并同步授权团队资产、查看本人用量与采集状态。用户不需要全局安装 TeamAI、Node.js 或用终端完成团队配置。

## Capabilities and Constraints

保留现有 Provider、Skills、Prompts、MCP、Agents、会话、OpenClaw / Hermes / Pi 个人功能。TeamAI 首版只接入固定版本的 Skill / Rule 资源适配及受控协议；实际文件写入仍由 NexusOps Rust 执行层完成。

组织和员工身份由服务端确认；Key 留在 OS 凭据存储。使用遥测仅包含 id、runtime、event、timestamp。配置、连接检测、Hook 注册和真实观察是不同事实。缺失数据不能表示成功率为 0 或系统运行正常。

## Brand Commitments

使用 NexusOps 名称和当前项目资产；保持可用的深 / 浅色、多语言和原生操作。SkillOps 截图与本地线框是布局参考，其中示例数据不进入产品。

## Evidence on Hand

执行规格：`docs/tasks/260910-client-teamai-refactor/task.md`，用户已要求按该文档执行。对应的基线、映射和合同保存在同一目录；测试与交付证据必须绑定当前版本。
