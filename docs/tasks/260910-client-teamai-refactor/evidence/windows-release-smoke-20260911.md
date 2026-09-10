# Windows 发布构建与隔离启动

2026-09-11，构建进程 81018 已成功结束，退出码 0。源码为 fff87c34 之后的未提交工作树，尚非版本固定的正式发布。

产物：`src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/NexusOps Client_0.1.0_x64-setup.exe`。

- 安装包 SHA256：`60ff5455c477b14cfb82546bce9765be60487eab1eeeb28efde4411fa1bfbdc4`
- 主程序 SHA256：`8fd35bf6a0cb3878b3bd6a73fe32af15dd47e4442086c5ac0a18f7217a2d8e55`
- 使用专用 TeamAI 构建配置，未签名、未创建 updater 签名产物；正式 updater 配置未修改。
- 构建生成的 NSIS 脚本包含 node.exe、worker.mjs、manifest 和许可证资源项；安装后的字节仍待验证。

实际启动的是 release 主程序，使用同目录 teamai 资源。子进程环境清空后只设置 Windows 系统变量、独立 TEMP/TMP、NEXUSOPS_CLIENT_TEST_HOME、CODEX_HOME、CLAUDE_CONFIG_DIR 和测试 WebView 调试参数；PATH 仅有 Windows System32，无全局 Node/TeamAI。没有使用原 CC Switch 实例，亦未改其配置。

验证结果：

1. 原生窗口首次启动正常，欢迎提示关闭后显示未连接组织、未选择供应商的真实空状态；WebView URL 为 `http://tauri.localhost/`，页面错误列表为空。截图 windows-release-first-run.png 来自真实发布版，不是 Vite/mock 页面。
2. 通过发布版真实 IPC 调用 teamai_run/version，返回 TeamAI commit `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`、version `0.22.0` 和受控操作列表；证明 release 资源解析及包内 Node/worker 可用。
3. 在独立中文/空格路径生成合成 Rule，再调用真实 teamai_export_candidate，成功生成 schema v1 候选（442 bytes），内容 hash `f06366cc96d50b6d60da96be630bb12c73718c8a6ffe4c88ea761f84d827ac2d`；页面无错误。

继续会话：测试进程 PID 49108，CDP 仅供当前本机测试使用的端口 15194。临时目录与启动参数记录于本机 TEMP/nexusops-package-smoke-session.json；后续操作需先确认该进程仍是本次测试程序。测试结束应关闭本次进程。

尚未验证：运行 NSIS 安装器、安装/卸载与升级、真实 CLI Hook、真实组织连接及 UAT。此记录不等于 T21/T23 全部通过；UAT 仍是旧版本且托管部署检查有账户运行条件问题。
