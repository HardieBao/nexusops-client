# Windows 标题栏与 74eddefe 安装复测

日期：2026-09-12。客户端 `74eddefe179d425bef73f0d1328ef0d35675164b`，服务端配对 `23a5657416db57938b78cf2c86a4533153a8220e`。本轮仅验收下述具体路径，不勾选 T03 / T10 / T21—T23 整体完成。

## 关闭白屏探针的原因

使用另一套 Windows Computer Use（@oai/sky）读取相同 release 窗口，发现两个不同的关闭控件：

- WebView 内部区域：`区域 NexusOps Client - Web 内容 → … → 按钮 关闭 ID: view_7`（本次 index 15）。
- 实际主窗口非客户区：`标题栏 → 按钮 关闭`（本次 index 68）。

之前 Orca 返回的关闭控件属于第一组，操作破坏了内嵌 WebView；它不代表用户点击 Windows 标题栏。不能复用这些瞬时 index，复测须重新读取树并按父级区分。

本次点击第二组的原生标题栏关闭：窗口隐藏，工具刷新报告窗口不可捕获；随后独立 Tauri IPC 读到 `is_visible=false`，同时仍能读取原资产库页面文本。没有丢失 WebView。

再次执行同一 EXE，第二进程 32348 正常退出（exit 0）；原进程 24264 保留，`is_visible=true`，页面内容完整。由此关闭 / 单实例唤回的实际原生路径通过，先前白屏记录归因为自动化控件定位错误。生产代码未修改。

## 深链确认与取消

用同一已运行 release 的第二实例传入合成 `nexusops://v1/import?resource=provider&app=claude` 链接（名称 Native DeepLink QA，example.invalid 地址，无有效密钥）。第二进程 66768 exit 0；原窗口出现“确认导入供应商配置”，保留取消和导入按钮。点击取消后返回资产库。

此项证明运行中单实例参数进入确认流程、取消可返回；没有执行供应商导入，也不证明所有资源类型的深链或系统浏览器协议分发均通过。托盘菜单未被本轮接口列出，尚未实际验证。

## 最新安装包实际执行

- 包：`NexusOps-Client-0.1.0-74eddefe-x64-setup.exe`；SHA256 `aac6cf35b70e1d0ee38b9d46086224e9340f5c241d106cc2dd6bafc0f90bf3e7`。
- 无既有 NexusOps Client 卸载记录、无活动测试客户端。先备份本产品协议注册、安装位置和同名快捷方式，不读取其他应用凭据。
- 使用 `/S /D=<独立中文空格路径>/安装 目录` 实际安装，exit 0。卸载记录 DisplayVersion=0.1.0，InstallLocation 与指定目录一致。
- 12 个 TeamAI 运行资源与构建清单逐一 SHA256 相等。安装 EXE 仅有 Tauri 打包标记从 `__TAURI_BUNDLE_TYPE_VAR_UNK` 到 `__TAURI_BUNDLE_TYPE_VAR_NSS` 的三个字节替换，其余全部字节一致；安装 EXE SHA256 为 `53983c14ea80a6349b53935e24709c119d1835bf3ad042132bcb0dec65a9f409`。
- 从安装目录启动实际程序，独立数据 / 工具配置 / WebView 目录，PATH 只有 Windows System32。首次欢迎确认后导航与工作台可用；截图见 `installed-74eddefe-first-run.png`。
- 实际 `teamai_run` 完成 version、inspect_skill、convert_rule:codex、convert_rule:claude-code。版本为上游 0.22.0 / `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`；合成 Skill / Rule 返回预期类型、名称和文件集合。页面 errors 命令未返回错误。
- 使用正常应用退出 API，确认进程结束，再实际运行卸载器 `/S _?=<安装目录>`，exit 0；程序和卸载记录移除，隔离数据库保留。
- 卸载器按“不删除应用数据”语义保留安装位置键，`_?=` 方式也留下卸载器自身。核对安装位置精确值、无额外子键 / 值，以及目录仅剩 uninstall.exe 后，清理这两项自建测试残留；没有启用删除应用数据。
- 原协议注册导出文件哈希、同名快捷方式、开机启动项与测试前一致。详见 `installed-74eddefe-check.json`。安装目录已清理，隔离测试数据库保留。

## 未覆盖范围

这仍是现有 Windows 用户下的隔离安装，不是全新 Windows VM。未连接真实组织，未执行该包的 UAT Hook 注册、资产审批安装、工具交互和管理员统计对账；这些硬性验收保持待完成。原生托盘菜单、其余旧功能入口与完整用户交互也不因本轮通过而自动勾选。
