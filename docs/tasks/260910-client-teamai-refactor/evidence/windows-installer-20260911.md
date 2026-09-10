# Windows NSIS 安装与卸载验证

2026-09-11，使用安装包 SHA256 `60ff5455c477b14cfb82546bce9765be60487eab1eeeb28efde4411fa1bfbdc4`。

在独立中文/空格目录执行实际 `/S /D=...` 安装。安装前停止的是本任务 release 测试进程；备份原 nexusops 协议注册、产品安装位置记录与同名快捷方式。原 CC Switch 进程 38040 未停止。

安装完成事实：程序、卸载器及资源落盘，Add/Remove Programs 记录版本 0.1.0 和正确目录。安装后的 12 个 TeamAI 资源（Node、worker、manifest、许可证）与构建资源逐字节一致。

主程序最初整文件 SHA256 比较失败，实际排查只发现 3 字节差异：`__TAURI_BUNDLE_TYPE_VAR_UNK` → `__TAURI_BUNDLE_TYPE_VAR_NSS`。本机 tauri-utils 2.8.3 的 platform.rs 明确说明此变量在构建时进行二进制修补，NSS 对应 NSIS。将内存中的比较基准做该唯一标记替换后，所有字节一致；未修改实际 EXE 或绕过内容验证。

- 构建目录 EXE SHA256：`8fd35bf6a0cb3878b3bd6a73fe32af15dd47e4442086c5ac0a18f7217a2d8e55`
- 安装目录 EXE SHA256：`30f5c96795bacf5e4f18d8a18e93ca53df0cb91a7b6736c67c509e55443f2225`

启动实际安装目录程序（测试 PID 51060），保持隔离主目录、工具目录及仅 Windows System32 的 PATH。原生 WebView 正常，实际 `teamai_run/version` 返回固定模块 0.22.0 / commit 6ae0619d…，无页面错误。截图 windows-installed-first-run.png 来自安装后的真实程序。

验证后停止测试进程并执行实际静默卸载；程序与卸载记录移除，独立测试数据库保留。原协议和安装位置注册以导出文件哈希核对恢复，旧快捷方式按文件哈希恢复；不存在的原快捷方式保持不存在。上游 CC Switch 仍运行。此前 PID 49108 与本次 51060 均已结束，不能继续作为活动会话使用。

边界：此为现有 Windows 用户环境中的隔离目录安装/卸载，不是新 Windows 用户或干净虚拟机，也未证明旧正式版本升级。未进行真实组织/CLI/UAT 联调，不将本项扩写为 T21—T23 全部通过。源码仍为未提交工作树。
