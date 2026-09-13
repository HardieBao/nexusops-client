# UAT 核心链路实测 — 2026-09-13

结论：本轮打通真实 UAT 的候选发布、两工具安装 / ACK、Hook 上报与管理员页面查询。发现并修复缺少私有对象存储造成的 Skill 上传 503。**这不是 T00—T23 / S01—S30 全部验收通过声明。**

## 版本及身份

- 客户端：`ce86a64e55f39011b823e9b8ad2a5c45bfc0d39c` 对应 release EXE，SHA256 `5fb08135173ebde994f70a45d40cf2954c6ee6cbab222358e90547710f2b8b45`。本轮通过真实 Tauri IPC 调用；该版本安装包的实际安装验证见 [交付记录](package-and-native-ce86a64e.md)。
- UAT 网关仍为 `23a5657416db57938b78cf2c86a4533153a8220e`，仅加载新增存储配置；重建前后 Docker image ID 一致，最终 health=ok、entitlement=ready。
- 存储部署配置提交：服务端仓库 `d8873b65ed6d00e4f5de25e15ceadd31255f7591`。不能将该配置提交冒充网关二进制 revision。
- 管理员 7，成员 A=8 / B=9，专用测试组 11，Key 24 / 25；来源见 [身份记录](uat-test-identities-20260913.md)。凭据仍在本机私有目录，不在此文档或 Git 中。

## 修复的部署问题

管理员创建 Skill 资产 1 成功，但上传 revision 返回 `503 ASSET_STORAGE_UNAVAILABLE`。源码确认网关未配置 S3，object store 为 nil；文本资产不依赖该存储。

在现有 `compose.uat.yml` 中增加私有 SeaweedFS 4.46，镜像固定为 `chrislusf/seaweedfs@sha256:08d516132314207d10c8e37cbffc1f32b147d870169688734cc61c6231625b62`。专用 internal 网络只有存储与 workspace，宿主端口绑定为空；网关凭据仅能读写专用桶。具体初始化、备份和回滚见服务端 `infra/vps/UAT-ASSET-STORAGE.md`。

- 有效凭据 Put/Get 通过，匿名读取 403，网关创建其他桶 403。
- 真实重启后原对象内容与 SHA256 一致。第一次检查在存储恢复前读取失败，脚本按预期恢复旧网关配置；等待实际对象可读后重试通过。未把第一次失败隐藏成成功。
- rollout 持有原部署锁，备份 Compose、网关 image ID、workspace 数据库；`pg_restore --list` 验证备份。未删卷，未重启其他业务服务。
- 最终存储 healthy，内存实测约 134 MiB / 512 MiB，网关约 51 MiB / 320 MiB。
- `bash -n`、UAT 部署合同、3 项镜像维护测试及备份报告行为测试通过；Compose `config --quiet` 和真实运行验证通过。GitNexus 工作树未注册、主仓库索引过期，context / detect_changes 报 Repository not found；已使用源码与上述针对性验证补足，未宣称图检查通过。

## 发布、下载及安装

两个候选由该 EXE 调用内置固定版本 worker 导出。管理员通过正常登录、TOTP 与 step-up 调用真实 UAT 管理 API，不绕过审批。

| 检查 | Skill | Rule |
| --- | --- | --- |
| 资产 / revision | 1 / 1 | 2 / 2 |
| 上传 | 201 | 201 |
| 发布前成员下载 | 404 | 404 |
| 未经过 Canary 发布 Stable | 409 `ASSET_CANARY_REQUIRED` | 同左 |
| Canary 发布 | 200 | 200 |
| 未审批发布 Stable | 409 `ASSET_APPROVAL_REQUIRED` | 同左 |
| 记录批准 / 发布 Stable 到组 11 | 201 / 200 | 201 / 200 |
| 成员下载 | 200 | 200 |
| Codex / Claude Code 安装 | 均 installed | 均 installed |

每个工具各 2 个 ACK 成功，waiting=0，无安装错误。四项落盘内容核对通过。Skill 经服务端规范化，不能比较原始 gzip 字节；实际核对文件集合、逐文件大小与 SHA256。Rule 核对原始 payload 字节。

- Skill canonical hash：`e0831cbad2fabde802110139ae59238e8b388d0c94f14652b5f1342a7dbf7364`。
- Skill 下载归档 SHA256：`de5d09da1b797a45d3954ff2c0c15c5795943342eb695cc680d9e01cb3719580`；其中 `SKILL.md` SHA256：`5e6ec08173c63144dd52e19bf703bec0986338e7b68a6ce8da0c1e0b1eaa22b1`。
- Rule SHA256：`1866c0ab5dc90d6d4338de75751b61a68385efb7d13b32763723e71d372787ea`。

## 统计、身份与真实工具

1. 向 release EXE 的正式 collector 输入 20 条**合成生命周期事件**，每工具 10 条；这部分不冒充真实 CLI 操作。落库 / 上报仅含 `id/runtime/event/timestamp`，输入中的测试提示词、路径、session ID 哨兵均未进入事件。
2. 正式客户端上传到 UAT 后 pending=0，本机历史仍为 20。管理员每工具读取：sessions=2、prompts=2、turns=4、tool_calls=2，合计两工具 20。分页 `total=2` 是聚合行数，不是事件数。
3. 原样重传其中 5 个 event ID，接口 204；服务端聚合完全不变。禁用采集后再次调用 collector，无新增记录。
4. A → B 后 B 的本机历史为空、采集默认关闭；B 新增 1 条 Claude 会话，仅归属员工 9，A 服务端数据不变。切回 A 后旧历史先隐藏；再次启用时通过 projects 确认 member_id 后恢复 20 条。当前存在“重新确认身份依赖启用采集”的体验限制，尚未修改。
5. 真实 Codex 与 Claude Code CLI 各运行成功，分别新增 SessionStart / UserPromptSubmit / Stop 共 3 条，通过同一正式 collector 和客户端上传到 UAT；两个 CLI 请求均包含已安装 Rule 的正文。**模型回答来自本机确定性测试服务，未验证 UAT 上游模型调用。** Codex 仅在隔离夹具中信任已检查的测试 hooks，没有修改日常工具配置。
6. Claude 第一次测试因本机夹具没有接受 messages URL 查询参数而失败；修复夹具路由后通过，未改产品。失败运行观察到的 2 条事件在测试禁用时清空待传队列，本机历史按设计保留。因此附加 CLI 演练后 A 本机为 28、服务端为 26；该现象不覆盖第 2—3 步已完成的同批 20 / 20 对账，亦不将失败运行算成完成轮次。

## 管理员 / 成员页面

管理员在浏览器完成邮箱、密码和六位 TOTP 正常登录，进入“员工工具使用”。员工 ID=8，分别选择 Codex / Claude Code，均只有对应员工 / 工具一行，与服务端结果一致；页面未捕获到 JS 错误。

- [Codex 筛选截图](uat-admin-codex-20260913.png)
- [Claude Code 筛选截图](uat-admin-claude-20260913.png)

成员 A 正常登录后直接访问管理统计路径，被路由带回 `/dashboard`，成员导航无管理统计入口；使用该浏览器的成员 JWT 请求统计 API 返回 403。两个成员 Key 访问管理统计都为 401。最终两名成员余额仍为 0；成员模型用量页没有模型调用记录。此轮未连接付费 UAT 模型。

## 未关闭的验收

本轮主要补足 T13—T18 / T22—T23 的正向 UAT 核心证据，不据此勾选整个任务。仍包括：资产管理 UI 的完整导入交互、全部撤权 / 过期 / 跨组织 / 故障联合场景、历史重新绑定的体验改善、托盘打开与退出、干净 Windows 用户或 VM、UAT 上游模型调用。原 task.md 的全部门槛继续有效。

测试组、账号和已批准的两个资产保留供人工 UAT 使用。临时客户端采集关闭，退出前断开连接并移除此次 OS Credential Manager 测试凭据；不改日常客户端数据。
