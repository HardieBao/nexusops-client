# 原生导出与候选包检查点

日期：2026-09-12；任务 T03、T12、T20、T21 的局部验收。客户端源码 `74eddefe179d425bef73f0d1328ef0d35675164b`；服务端交付配对 `23a5657416db57938b78cf2c86a4533153a8220e`。本记录不表示这些任务整体完成。

后续复测已确认下述原生关闭异常由选错 WebView 内部控件造成，并完成最新安装包的实际安装 / 卸载；见 [后续证据](windows-native-and-installer-20260912.md)。本文件保留早先观察，不把历史待验收描述当作最新状态。

## 原生文件选择与资源导出

使用实际 release EXE，隔离 NEXUSOPS_CLIENT_TEST_HOME、Codex / Claude 配置和 WebView 数据；PATH 只包含 Windows System32，未借用全局 Node / TeamAI。样例均为本次自建内容，不含个人或团队数据。

1. 上轮在真实 Windows 125% 缩放下通过 Rule 文件选择与另存为对话框生成候选，截图为 `native-picker-system125.png`。上轮已恢复系统 100%；本轮真实窗口 API 再次读到 scaleFactor = 1。
2. 本轮从资产库选择 Skill 目录，使用 Windows 原生目录对话框选中含 SKILL.md 的 `skill-input`，再用原生另存为保存候选。页面显示“候选文件已生成：native-skill-fixture”。见 `native-skill-picker-20260912.png`。
3. 读取两个实际候选，校验 kind 和固定上游 SHA。解码 Skill gzip/tar，在内存中读取 SKILL.md；文件长度、清单 SHA256、归档 SHA256 以及原始样例字节全部一致。没有将归档解压到用户目录。
4. 候选样例及结果分别保存在 `native-rule-candidate-20260912.json`、`native-skill-candidate-20260912.json` 和 `native-export-check-20260912.json`。Rule 本轮只重新核对候选元信息和文件哈希，不将其标记为本轮完整 payload 验证。

这证明本机原生选择器、内置 worker、候选生成与保存可用；不替代管理员导入、审批、Stable 发布、客户端安装或 UAT 验收。原生选择器首次显示旧目录列表，关闭重开后找到新建的样例目录；没有修改产品代码绕过对话框。

## 窗口关闭和单实例的证据边界

- 读取实际设置：showInTray=true、minimizeToTrayOnClose=true。
- 使用应用允许的 `plugin:window|close` 后，`is_visible` 从 true 变为 false，进程仍运行。
- 使用相同隔离环境再次启动同一 EXE：第二进程 16748 退出码 0；原进程 49524 保留，`is_visible` 回到 true，原资产库内容仍可读取。这证明关闭 API 与单实例唤回路径。
- **原生鼠标关闭仍未验收通过**：Orca UIA 树中的“关闭”操作后 WebView 消失，唤回呈空白窗口；系统快捷键操作也未证明窗口被关闭。现有证据不足以确定是控件定位 / 输入投递问题还是产品原生路径问题，不能用 API 通过掩盖此项。该异常测试实例经校验 PID 与 EXE 路径后终止。
- 首次单实例探针还触发 fern 日志 panic：父测试命令结束后，继承的 stdout / stderr 管道关闭（Windows error 232）。改为独立 DEVNULL 输出句柄后，同样的第二实例回调不再崩溃。此为探针运行方式的差异记录；未修改生产日志逻辑，保留原失败，不宣称已修复所有日志边界。
- 上轮标题栏拖动、最大化、还原、最小化有局部观察，但完整标题栏关闭、托盘菜单和深链仍需独立证据。

## Windows 候选包

构建日志显示 MSVC release 与 NSIS 均成功，生成时间为 2026-09-11。使用 `pnpm build:teamai:windows --bundles nsis --no-sign`，临时构建配置只关闭 updater artifacts，正式更新配置未改。

- 不可变副本：`src-tauri/target/deliverables/74eddefe/NexusOps-Client-0.1.0-74eddefe-x64-setup.exe`。
- 安装包 SHA256：`aac6cf35b70e1d0ee38b9d46086224e9340f5c241d106cc2dd6bafc0f90bf3e7`。
- EXE SHA256：`0651e9efd6f305a9682e42c0a3d681f12f05ca76fab302dd3fb671b1ffcf268e`。
- 12 个 TeamAI 运行资源文件逐一计算哈希，包含 Node、worker、manifest 和许可；完整记录见 `package-74eddefe.json`。安装包副本与原始构建包哈希一致。
- 本轮执行的是对应 release EXE。旧 4d33 安装包的实际安装 / 卸载结果不能替代新 74eddefe 安装包的安装验收；新包安装及 UAT 登录完整链路仍待完成。包为未签名试用包。

## 清理与剩余动作

两个自建 Documents / Downloads 样例目录在核对绝对路径、文件白名单和无重解析点后移除，已生成的脱敏候选保存在证据目录。未操作此前被自动审批拒绝清理的 collector-console-probe 程序。

剩余：原生关闭 / 托盘 / 深链、全部旧入口与交互场景、新包安装验收，以及使用授权 UAT 管理员与成员身份完成发布、安装和统计对账。task.md 的 O1—O6、T00—T23、M1—M5 和 S01—S30 范围保持不变。
