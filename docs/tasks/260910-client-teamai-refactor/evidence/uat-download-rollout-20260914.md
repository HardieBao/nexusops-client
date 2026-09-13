# UAT 下载入口上线 — 2026-09-14

Gateway（含工作区控制台）及 UAT 官网已通过本地构建的 Docker 镜像更新到 `13c6f4874397d534787eb5bbd10aaa4e3581243c`，两端公网健康接口均返回 200、目标 revision，工作区授权 ready。

- 官网导航出现“客户端下载（测试版）”。
- 已登录工作区控制台顶栏出现“下载客户端（测试版）”；实际点击后在新标签打开 [Client Release](https://github.com/HardieBao/nexusops-client/releases/tag/client-preview-0.1.0-ce86a64e)。
- 测试成员仍可读取原有资产目录，Skill / Rule 下载返回 200，内容哈希与部署前一致；成员 Key 读取管理员统计仍为 401。
- 三份数据库备份通过校验，平台接受备份报告；保留回滚镜像，其余运行中 UAT 容器未替换。

直接 SSH 仍不可用；本轮通过用户登录的加密 VNC 终端和项目私有 HTTPS 附件传输完成 Docker 部署。临时附件与服务器传输压缩包已清理。此结果不代表 GitHub Actions 计费问题、正式站点或全部客户端验收已经解决。

客户端安装包仍为 `ce86a64e`，没有因服务端下载入口部署而重新构建。[服务端详细记录](https://github.com/HardieBao/nexusops/blob/develop_hardie/docs/deploy/uat-docker-update-20260914.md)。
