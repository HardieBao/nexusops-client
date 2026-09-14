# 统一供应商添加入口与品牌图标修复

日期：2026-09-14。程序提交：`4d884bf0d372ef517bd6b6c58468657ec8010842`，已推送 `develop_hardie`。

## 问题和修复

- 统一供应商面板有表单和保存处理，但只有编辑操作能打开表单，空状态文案指向不存在的添加按钮。空列表补充“添加统一供应商”，已有数据时在页头显示同一入口；每个状态只有一个添加入口，复用现有保存与同步行为。
- 侧栏与关于页引用的 `src/assets/icons/app-icon.png` 仍为 CC 花形图标。将其替换为项目已有的 NexusOps N 图标，与原生安装包 `src-tauri/icons/128x128.png` 字节一致。原生图标已正确，未重新设计或改动工具供应商自身的品牌图标。

## 验证

- 修复前两个真实面板交互用例均因找不到添加按钮失败，品牌一致性用例失败；修复后相关 10 项测试通过（创建入口、真实表单保存同步、有数据时继续添加、取消、品牌和全屏面板）。
- `pnpm typecheck`、修改文件 Prettier 检查、`pnpm build:renderer` 通过。
- 真实 React 面板、表单和侧栏组合，在 1000×650、900×600 检查：添加按钮在首屏可见，创建后页头入口可用，无页面横向溢出，N 标识正常显示。使用隔离的内存 API 和合成数据；未写入用户真实工具配置。临时页面、服务和浏览器标签已清理。
- GitNexus impact 对面板和 ClientShell 返回 LOW，源码确认面板经 App 的统一供应商入口调用，图片由 ClientShell / AboutSection 引用。索引刷新因 LadybugDB WAL checkpoint 文件轮换失败，仍为旧索引；没有将旧图作为完整安全证明。修改后及提交前 detect-changes 为低风险，结合实际 diff、类型与交互测试确认范围。
- 本机默认 VS 预览版缺少 C 标准头文件，初次原生构建失败。改用已有完整 VS2019 MSVC 14.29 x64 开发环境，Rust 1.95.0 优化编译与 NSIS 打包成功；仅对本次构建进程设置编译环境。
- Windows x64 主程序 PE 架构验证通过，内置 TeamAI worker 哈希匹配固定构建清单。没有执行本包的全新 Windows 安装、真实组织或上游请求回归；本次不是完整跨平台发布验收。

## 测试版交付

[Release](https://github.com/HardieBao/nexusops-client/releases/tag/client-preview-0.1.0-4d884bf0) 中提供安装包、`build-info.json` 和 `SHA256SUMS.txt`。

[直接下载 Windows x64 测试包](https://github.com/HardieBao/nexusops-client/releases/download/client-preview-0.1.0-4d884bf0/NexusOps-Client-0.1.0-4d884bf0-x64-setup.exe)

- 文件：`NexusOps-Client-0.1.0-4d884bf0-x64-setup.exe`。
- 大小：33,767,776 字节。
- SHA256：`4214a41aacd374580148f69226a65dbb392b2b7d27ded702f5b3508e3a2c8a47`。
- 已实际匿名下载，HTTP 200，大小与 SHA256 和本地产物一致。
- 本地构建、未签名、手动安装的预发布包；未生成自动更新清单，未将 hosted CI 作为通过证据。旧测试版保留，当前固定下载链接不会自动指向此包。
- 本地产物目录：`src-tauri/target/deliverables/4d884bf0/`；构建和下载验证记录保存在本机私有 `nexusops-client-fix-20260914` 目录。
