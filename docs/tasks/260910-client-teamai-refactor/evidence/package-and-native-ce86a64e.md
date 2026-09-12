# ce86a64e 交付包与最终源码检查

2026-09-12；客户端 ce86a64e55f39011b823e9b8ad2a5c45bfc0d39c，服务端 / 管理端配对 23a5657416db57938b78cf2c86a4533153a8220e。目标仍为部分完成，不是完整 UAT 验收报告。

## 包与安装验证

VS 2019 环境运行 `pnpm build:teamai:windows --bundles nsis --no-sign --config <临时关闭 updater artifacts 的配置>`，实际 exit 0，Rust release 6m16s、NSIS 成功。构建期间只有验证文档变化，程序源码与提交一致。

- 包：`src-tauri/target/deliverables/ce86a64e/NexusOps-Client-0.1.0-ce86a64e-x64-setup.exe`。
- 包 SHA256：`5a43c3064c135a687dd77fb51236cfd054d6d7ae20ffad52c93ad5f62d23c7b7`。
- 构建 EXE SHA256：`5fb08135173ebde994f70a45d40cf2954c6ee6cbab222358e90547710f2b8b45`。
- 安装 EXE SHA256：`2b450f9ba81e9b8b1eed914477023d3c32291772b04ab1fac0a84d24ce2a57aa`。

实际安装到独立中文 / 空格目录，exit 0；12 个 TeamAI 资源哈希与固定清单一致。安装 EXE 仅有已核对的 UNK → NSS 打包标记差异。清单见 `package-ce86a64e.json`。

从安装目录启动，使用隔离应用、工具和 WebView 数据目录，PATH 仅 Windows System32。真实 worker 返回 0.22.0 / 6ae0619d… 与三项受支持操作；get_proxy_status.running=false。正常退出并实际卸载，exit 0；程序、卸载记录与本次安装目录清理，隔离数据库保留，协议 / 快捷方式 / 开机启动注册恢复至测试前。结果见 `installed-ce86a64e-check.json`。

## 四语言显示

每种语言通过保存偏好后正常退出、重新启动安装版验证。简中、繁中、英文、日文的实际副标题均精确匹配源码，明确本机模型请求、估算费用、Hook 分离与组织账单来源。

每种语言分别使用 1280×800、960×640 CDP 视口检查，合计 8/8：副标题完整、段落不横向溢出、页面宽度符合视口。此为原生 EXE 内 WebView 的 CSS 视口模拟，不冒称 OS 窗口缩放；系统 125% 检查仍见原独立证据。数据见 `usage-caption-ce86a64e.json`；四张 usage-caption-ce86a64e-*.png 来自真实安装版。英文小视口截图另作目视检查，说明正常换行，未遮挡筛选控件。

## 最终源码与性能

ce86a64e 全量前端检查：147 文件 / 1192 tests 全通过。四语言 JSON 结构核对、Prettier、typecheck、renderer build 和 6 项用量组件测试均通过；仍有既有大分块告警。Rust / Gateway 业务源码未变，不将旧测试当本轮新执行结果。

与 f811e352 同机、相同 Windows release 构建方式，各交替运行 5 次全新进程、空应用 / WebView 数据：

| 指标中位数 | f811e352 | ce86a64e | 比例 |
| --- | --- | --- | --- |
| 可交互观察时间 | 867.3 ms | 869.3 ms | 1.0023 |
| 进程树工作集 | 434,266,112 bytes | 426,631,168 bytes | 0.9824 |

两项未超过 20% 回归阈值。包含探针开销，3 秒稳定后求进程树工作集，可能重复计算共享页，没有清空 OS 磁盘缓存；不代表大数据 / 已联网组织场景或统计学上的性能改善。10 个样本与两个 EXE 哈希见 `performance-ce86a64e.json`。

## 剩余

尚需授权 UAT 管理员 / 成员身份完成发布、安装、真实工具上报与对应员工统计；托盘菜单操作已请求人工补验。现有 Windows 用户下的隔离安装不能替代新 Windows 用户 / VM。其余原 task.md 场景仍按证据逐项验收，不能用本包或性能通过宣布全部完成。
