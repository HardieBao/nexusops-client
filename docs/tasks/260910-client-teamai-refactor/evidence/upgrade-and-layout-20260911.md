# 升级、窗口行为与布局验收检查点

2026-09-11；本次新增的生产修复仅为首页组织名称换行规则。

## 实际升级读取

运行从 f811e352 源码构建的旧 release，在独立数据目录通过真实 IPC 创建合成 Claude 供应商（无有效上游凭据），设置用量刷新间隔 15 秒、关闭会话自动扫描。正常退出后，用 4d33d55d release 打开同一数据目录。

供应商完整元信息、当前供应商、上述偏好均保留；实际存在的 Claude settings.json 在升级启动前后哈希不变。两次均通过应用 exit API 正常退出。结果及两个二进制哈希见 upgrade-f811-to-4d33.json。此项是应用读取旧配置，不等于所有旧版本 NSIS 升级路径都已验证。

## 窗口与键盘

真实 release 通过已允许的窗口接口完成最大化、还原、最小化、恢复，读取状态符合预期；系统 scaleFactor 实测 1.0。记录见 native-window-check.json。未声称鼠标点击原生标题栏、拖动和托盘全部完成。

在独立前端夹具中以日文、暗色、960×640 和 deviceScaleFactor=1.25 运行：Tab 聚焦工作台导航，:focus-visible 为 true、outline 为 solid；Enter 实际进入工作台。截图 keyboard-dpr125-emulation.png。这是 WebView DPR 模拟，不是 Windows 系统 125% 缩放验收。

## 长名称与语言布局

使用实际 App 组件和合成 IPC 数据：160 字符无空格组织名、156 字符资产名。六个页面（home、team、teamAssets、teamProviders、assetLibrary、usage），四种语言（简中、繁中、英、日），1280×800 浅色和 960×640 暗色，共 48 个场景。

初次发现首页组织名溢出（8 个场景）；新增 break-words 修复。检查脚本的正则转义也已修正。旧预览服务缓存导致一次复测仍显示旧 class；新建预览服务后验证的是磁盘当前源码，最终 48/48 通过，检查页面宽度、未裁切文本溢出、主题、实际语言标题和裸翻译 key。数据见 layout-matrix-48.json，修复截图 long-organization-fixed.png。

相关 typecheck、1181 项完整前端测试、renderer build 通过；GitNexus upstream 为 LOW，修改后 detect_changes 确认仅此生产函数变化。没有为单个 CSS 类增加只重复实现的单元测试。

剩余：真实系统 125% DPI、完整原生标题栏/托盘/深链/文件选择器操作，以及已授权 UAT 登录后的发布、同步与统计对账。最新换行修复需进入下一候选包，不能沿用 4d33 包的哈希作新版本证明。
