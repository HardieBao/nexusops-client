# CLIENT-TEAMAI 首版合同 v1

状态：实施合同。界面、Rust、TeamAI worker 和 Gateway 以本文件为共同边界；新增字段 / 指令必须修改合同并添加反例测试。原 `team-profile` v1、资产接口和工具使用事件接口继续兼容。

## D01：导航与任务边界

采用 `feature-mapping.md` 的全部旧 View 映射及六个稳定入口。没有保存值首次进入 home，未知值回 home；合法旧值保持原功能。默认不自动连接、启用 Provider、安装资产或打开统计。所有新导航均复用管理操作阻塞状态。

## D02：组织、员工与项目编号

首版每个 Gateway 部署只有一个当前工作区。兼容合同的 `project_id` 固定为整数 1，由服务端映射到该部署的组织 / 工作区，不创建项目业务实体。只有已认证的当前成员能读取该映射，其他 ID 返回 404。

作用域为 `(organization_id, workspace_id, authenticated_user_id)`；不同部署的 project_id 都可以为 1，但它不是授权凭据。禁止从 cwd、主机名、配置文件或请求体中的 user_id 推断员工。客户端通过新 projects 响应取得稳定 member_id；旧 profile v1 不增加破坏 `deny_unknown_fields` 的字段。跨身份不可复用旧确认或统计；同一身份更换 Key 只有在重新验证稳定 member_id 后才复用本机聚合。

## D03 / D04：TeamAI 模块与内部 IPC

固定上游 `6ae0619d067b1699bb2c6e435abf3ffe11a21d71`。首版采用资源格式 / frontmatter 等受选模块，不导入 CLI 主入口、init、pull、dashboard、session collector 或 Git push。Skill 与 Rule 为新融合资源；Prompt 保持既有通道。

worker 是受控资源转换子进程，结构化 stdin / stdout，UTF-8、一个请求对应一个结果；不解析 CLI 人类日志，不接受任意命令名。首版仅支持 `inspect_skill`、`convert_rule`、`version`。取消 / 超时必须终止当前 worker 并回收管道；实例不持有网关 Key，不直接访问网关。

```json
{"schema_version":1,"request_id":"uuid","operation":"inspect_skill","input":{"root":"用户明确选择的临时资源目录","entry":"SKILL.md"}}
```

```json
{"schema_version":1,"request_id":"同一uuid","ok":true,"data":{"kind":"skill","name":"sample","entry":"SKILL.md","files":[{"path":"SKILL.md","size":120}]}}
```

文件正文按资源导入需要读取，但 worker 无权将内容写入工具目录。相对路径不得含 `..`、绝对路径、盘符、链接越界、保留设备名、大小写 / Unicode 归一化冲突。Rust 对实际文件重算哈希，worker 提供的路径 / 大小 / 哈希不被盲信。

输入最多 1 MiB，stdout 最多 4 MiB，诊断最多 8 KiB；单次默认超时 15 秒。资源大小沿用现有服务端下发的资产 limits，不单独放宽。错误仅返回固定 code 与安全描述：`invalid_input`、`unsupported_operation`、`unsafe_path`、`too_large`、`cancelled`、`timeout`、`worker_unavailable`、`invalid_response`。未经审查的异常堆栈、路径或资源正文不得显示为诊断。

Windows 包使用固定受支持的 Node 运行时及已构建 worker；绝对可执行文件路径，不依赖 PATH，也不把成员 Key 放入 argv / environment / token 文件。版本清单记录 Node、模块 SHA、许可、worker 及必要 WASM 的哈希。具体构建产物结构在 T11 固定，但不得改变这些运行合同。

## D05 / D06：审批与实际执行

管理员将 TeamAI 候选转换后通过现有资产管理 API 创建不可变 revision，继续执行 Canary → 人工审批 → Stable。普通成员没有该写权限。TeamAI 来源信息仅为来源，不是权限或批准证明。

资源最终写入统一调用现有 Rust 安装器：预览 → 明确确认 → 校验当前授权 / 哈希 → 原子安装 → 保存基线 / 恢复日志。进入页面、连接组织、SessionStart 以及网络恢复均不得自动 Git pull 或覆盖个人配置。

首版仅允许 `install_skill` 与 `install_rule` 指令。`apply_model_config`、插件、MCP、Hook 修改、客户端卸载与所有未知 type 均返回 unsupported，不进行降级猜测执行。

## D07：工具状态

三条互不替代的事实：

- 配置：`missing / configured / invalid / unsupported`，来自当前实际配置读取。
- 连接检测：`not_checked / checking / reachable / failed / stale`；只在用户主动检测后写入，成功超过 15 分钟显示 stale。展示检测时间，不称为持续在线。
- Hook：`not_installed / installed_unobserved / observed / stale / unsupported / disabled`；注册检查来自本功能 marker，observed 来自最近真实 native event。最后观察超过 24 小时显示历史时间 / stale，不解释为工具故障。

网络离线、鉴权失效、组织暂停是组织连接状态，不伪装为工具未安装。未知数据不填 0%、绿色正常或估算成功率。

## D08 / D09：本机聚合与上报

使用事件保持严格的 `{id,runtime,event,timestamp}`。runtime 为 `codex | claude-code`；event 为现有五种枚举；id 为 64 字符小写十六进制；timestamp 为带时区 RFC3339，UTC 存储。

本机将已采集事件的去重键 / 最小日聚合与 outbox 分开，按稳定组织 / 工作区 / 员工隔离。日聚合以 UTC 日期计数，界面明确标注；不与本机模型请求的既有时区策略混用。保存 30 天，清理按 UTC 边界；不保存原始 session ID、提示词、工具入参、路径或助手输出。

outbox 继续每分钟最多 200 条、最多 10,000 条，HTTP body 64 KiB；重试保留 event id。ACK 删除 outbox 后不回减历史。本机去重键与聚合一起保留，避免重启 / 补传再次累加。

关闭：停止新增聚合和 outbox，清空未上传；已聚合本人历史可保留到期或手动清理。退出：停用、清空待上传、解除身份展示；另一员工不能查询此前记录。再次连接不自动开启统计。当前四字段以外的元数据不得混进统计 POST。

## D10：Gateway 兼容层

命名空间：`/api/v1/me/teamai`。仅 NexusOps 内部桥接协议 v1，不宣称能运行未修改的完整 TeamAI CLI。

鉴权沿用 Rust 发送的唯一 `X-Nexus-Member-Key`；拒绝同请求混入 Bearer、X-API-Token、x-api-key 或 query 凭据。身份、IP ACL、成员状态、分组授权、工作区租约均由现有控制面认证校验，不使用模型余额 / 消费作为资源门禁。响应沿用 Gateway envelope；失败 code 不包含凭据或正文。

| 方法 / 路径 | 输入 | 结果与副作用 |
| --- | --- | --- |
| GET `/projects` | 无 | `{schema_version:1,projects:[{id:1,name,organization_id,workspace_id,member_id}]}`；只读当前作用域 |
| GET `/config` | `project_id=1` | 首发工具 / 资源 / schema 能力；无模型密钥、插件配置或远程可执行配置 |
| POST `/report` | `{schema_version:1,project_id:1,runtime,bridge_version}` | 最小兼容心跳确认与 server_time；不接收设备名、工作区路径、资源全量盘点。使用统计仍走既有 `/me/tool-usage` |
| POST `/sync` | `{schema_version:1,project_id:1,runtime,asset_ids?}` | 从现有授权 stable manifest 派生安装候选和不可变 command_id；可选 asset_ids 最多 200 个唯一正整数；不写客户端文件 |
| POST `/ack` | `{schema_version:1,project_id:1,command_id,runtime,outcome}` | 仅确认当前用户被签发的指令；幂等记录结果，不据此修改资产版本或模型计费 |

`bridge_version` 是最多 32 字符的产品版本标识，不得包含机器标识或任意自由文本。report 只提供协议状态确认，不用它制造未验证的实时员工在线状态。

sync 返回的每个候选包含 asset_id、revision_id、kind、runtime、content_hash、下载引用、issued_at、expires_at；沿用 manifest 的资源大小 / 文件数上限。每次最多 200 个候选；未指定 asset_ids 且符合条件的候选超出 200 时返回 413，不静默截断。客户端可通过既有授权资产目录选择不超过 200 项再请求；任一显式指定 ID 不在授权范围内则整个请求返回 404，不能静默跳过。

command_id 为作用域 + member_id + asset_id + revision_id + runtime 的规范化元组 SHA-256。服务端使用最小的指令签发 / 确认记录保存这些事实和 30 分钟有效期；不建立通用后台任务引擎。重复 sync 仅在重新校验当前授权后续签同一候选，已完成 outcome 不重置。数据只进入 Gateway 所属迁移。

ack 的 outcome 为 `applied / already_current / conflict / skipped / cancelled / failed`。同一成功结果可重复 ACK；不能把成功状态降级为失败或用新的 revision 覆盖旧签发记录。未知、过期、撤权、换用户或候选已不再为当前 stable 时拒绝执行 / 确认；客户端重新同步后，只有仍授权的同一 revision 可续签并补 ACK，否则将旧操作标为 superseded，不能无限重试。

服务端 receipt 唯一作用域包含 organization、workspace、member 和 command_id。客户端安装成功但 ACK 失败必须保持本机安装基线，只补确认；不得重新执行覆盖。

## 错误、缓存和反例

- 400：非法 schema、字段、runtime、混用身份、未知枚举；未知字段严格拒绝。
- 401：成员 Key 缺失、失效、过期；不重试原凭据到其他 origin。
- 403：成员停用、组织暂停、IP / 分组拒绝；保持本机文件，禁止继续受保护网络操作。
- 404：项目 / 资产 / 指令不在可见范围；不给出其他组织是否存在的提示。
- 409：状态冲突、过期 / superseded 指令、哈希或预期版本不符。
- 413：传输 / 资源超限；429：按 Retry-After 和有上限退避；503：依赖故障。
- 身份相关 API 使用 no-store；禁止跨用户共享 manifest / projects / receipt 缓存。下载拒绝未经授权的跨 origin 重定向。
- 例：员工 B 将 A 的 command_id 发送到 ack → 404，无状态变更。
- 例：合法 revision 的内容被替换 → 客户端 hash mismatch，无写入成功状态。
- 例：applied 后 ACK 丢失 → 重试同一 ack 返回已确认，不重新安装。
- 例：`report` 携带 `host_name`、`cwd` 或 `prompt` → 400。
- 例：worker 返回 `../../.ssh/config` → unsafe_path，所有目标文件保持原样。

## D11：部署与证据

UAT 当前健康响应已经是基线 9a3022a0；未来新版本必须记录实际源码 SHA、镜像 digest、迁移头和客户端包哈希。优先现有部署通道；任何直接部署仍要完成同版本测试、备份、健康检查和回退。Git 推送、旧 CI 结果或本地 mock 都不是新功能 UAT 验收。

本合同是设计与实现边界，真实请求 / 响应夹具、数据库约束和协议测试由 T12—T19 逐项实现后附入 evidence。合同存在不能单独证明这些接口已上线。
