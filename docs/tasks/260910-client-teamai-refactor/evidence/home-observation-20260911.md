# 工作台工具状态与 Windows 打包启动

2026-09-11，未提交工作树；未完成 M5。

工作台读取当前连接的工具观察接口，分别显示 Hook 注册、采集和最近事件状态，提供组织上报与使用记录跳转。查询按 connection ID / Key ID 隔离，卸载不保留观察缓存；首页不写配置、不启用采集、不执行修复。禁用 Team 或未连接不请求观察接口。

检查：typecheck、147 文件/1179 项完整前端测试、renderer build 均通过。新增首页测试验证实际状态文案及跳转。浏览器 home-observation 使用实际 App 和 IPC 合成夹具，读取及跳转到使用记录正常，无页面错误；截图 home-tool-observation.png 中状态是合成案例，不是实际员工或 UAT 统计。修改前 GitNexus upstream LOW，累积 detect_changes 和 diff 检查已执行。

已启动 Windows x64 NSIS 实际构建：MSVC 2019 + `pnpm build:teamai:windows --bundles nsis --no-sign --config <临时试用配置>`。临时配置仅设 bundle.createUpdaterArtifacts=false，保留专用 TeamAI 资源配置；未修改正式 updater 配置。日志：本机临时目录 `nexusops-teamai-windows-package.log`。此记录仅证明构建启动，不能作为安装包成功或安装验收证据。

剩余：确认构建结果与包哈希、内置资源清单、隔离启动、真实 Hook/CLI 流程，以及对应 Gateway/Console UAT 部署与回退。
