# 上报面板保护与 Gateway 镜像

2026-09-11；整体目标未完成。

## 上报界面

- 旧 Gateway 返回 upgrade_required 时明确提示升级；认证、权限、限流使用固定文案，未知错误码走通用提示，不直接展示原始消息。
- 刷新请求带版本号，不能覆盖之后启用/禁用的结果；操作中及页面不可见时不持续轮询，卸载后结果失效。
- TeamPage 的上报面板按 connection ID + Key ID 重建，避免同工作区更换身份后沿用旧状态。
- 两个新增回归先失败后通过；面板 6 项测试通过，完整前端 147 文件/1181 项通过，最终类型检查与 renderer build 通过。错误码白名单最终调整后相关 6 项再次通过。
- 浏览器 usage-upgrade 使用实际 App 和合成 IPC，出现明确升级提示，原始 PRIVATE_FIXTURE_DETAILS 未显示，页面 errors 为空。截图 usage-upgrade-guidance.png 为夹具。GitNexus 相关组件 upstream LOW，修改后 detect_changes 与 diff 检查已执行。

这些 UI 改动晚于安装包 b6524f50…，最终客户端交付需重新打包，不将旧包称为包含全部最新改动。

## Gateway 制品

- 源码由干净提交 23a5657416db57938b78cf2c86a4533153a8220e 的 git archive 提供，未包含工作树或本机环境文件。
- 首次构建因 Alpine APKINDEX 临时下载失败；重新检查索引 HTTP 200 后重试。第二次因 PowerShell JSON 日期自动转换使 DATE 含空格而链接失败；恢复 UTC ISO 字符串后同 SHA 构建成功，未改业务源码或关闭 TLS 校验。
- OCI 镜像索引摘要：sha256:8d78cfa6843e41a1c0847a8b1902e55086713e806afdb9a34506c3a89bbe031a；linux/amd64 manifest 为 sha256:4976a925a9907e5119502a9aa84727c34ed7b63a55f12cded70cfe325737414d，config 为 sha256:456548e349da1e7b8b3beb88e314d373bee5cec754c817bd227b9fa6913246ea。
- 容器实际 `/app/nexusops -version` 输出版本 uat-23a56574…、对应完整 commit、日期 2026-09-10T20:31:13Z。
- linux/amd64 docker save 归档 SHA256：48339e93e62c04834216949daee10e066a2c503a507c1ec33a1c80cea2cd5e8d。
- 已启动向 UAT 独立 teamai-artifacts/23a56574… 目录传输。上传/远端哈希与加载结果需后续记录；没有切换服务、应用迁移或宣称 UAT 联调通过。

新 release EXE（7f5520dc…）已单独重新运行真实 Codex/Claude 采集测试，两种工具各 3 个事件，ACK 后历史保留，2 项通过；仍使用合成模型响应，不替代 UAT 真实模型。

### 远端加载结果

SCP 传输完成；远端归档 SHA256 与本地 48339e93… 一致后才执行 docker load，成功加载 nexusops-teamai:git-23a5657416db57938b78cf2c86a4533153a8220e。远端 image inspect 返回 linux/amd64 manifest 摘要 sha256:4976a925a9907e5119502a9aa84727c34ed7b63a55f12cded70cfe325737414d，OCI revision/version/date 标签与本机构建一致。它不同于本机多平台索引摘要，不能混写为同一层级的 ID。

这一步只加载镜像，未修改 compose 配置、重启服务或应用迁移。备份、受控切换、健康检查及回退仍待执行。
