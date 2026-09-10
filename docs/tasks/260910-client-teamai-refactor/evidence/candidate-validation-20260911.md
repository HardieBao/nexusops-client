# T13 候选校验检查点

日期：2026-09-11。结论：实施中，未完成发布闭环。

## 已核对的客户端结果

读取已有实际执行日志，未将本次文档复核冒充重新运行：

- `teamai-candidate-frontend-full.log`：146 个测试文件、1175 项通过。
- `teamai-candidate-native-full.log`：2867 项通过、16 项默认跳过。
- `teamai-candidate-renderer.log`：构建成功，仍有分块大小告警。
- `teamai-candidate-disabled.log`：no-default-features lib check 成功。

日志位于执行机器临时目录；这些结果对应当时未提交的客户端工作树，不是发行包或 UAT 验收证据。

## 本轮服务端改动

位置：nexusops 工作树 `.worktrees/team-ai-control-plane`，新增 `internal/asset/content/candidate.go` 与测试文件。

- 读取客户端 schema v1 候选结构，限制 JSON 为 32 MiB，拒绝未知字段、尾随 JSON、错误 base64、类型错配和不合法来源字段。
- Rule 必须是规范化文本且哈希匹配；Skill 重新执行现有归档规范化与路径验证，比较文件清单、内容哈希和原始归档哈希。
- 来源仅是声明的转换元信息，不作为作者身份或发布权限证明。没有把转换模块 commit 当成资产 Git commit。
- 解析器无持久化或发布副作用，尚未接入上传服务。

本轮实际运行：

```text
go test -count=1 ./internal/asset/content   PASS
go build -mod=readonly ./cmd/server       PASS
go vet ./internal/asset/content           PASS
git diff --check                         PASS
```

测试覆盖两种资源、正文/哈希/来源/版本/类型/清单篡改、JSON 和内容上限，以及“非法路径归档具有正确归档哈希”仍被拒绝。不是 Rust 导出到 Go 导入的跨语言端到端证据。

GitNexus：status 返回主仓库 develop_hardie 的过期索引（54ac6be，当前 9a3022a），未覆盖本工作树；query 无相关调用流程，detect-changes 显式指定工作树返回 Repository not found。不能据此证明影响安全。本轮仅新增解析器及测试，复用既有 NormalizeText / NormalizeSkill，未修改既有生产函数；以源码和包测试补足，调用链图仍存在覆盖缺口。

## 剩余验收

1. 上传服务实际调用解析器，权限校验在内容处理之前执行。
2. 来源与 revision 一起持久化，不可变并可供管理员查看。
3. 管理员表单预览并导入候选文件，保留原始文件上传。
4. 实际客户端导出文件跨语言验证；Canary、审批、Stable、成员可见性和安装闭环。
5. 对应版本 Windows 包与 UAT 联调。

T13 / M3 未通过，不调整其他场景的完成条件。

## 后续检查点：候选上传与来源持久化（2026-09-11）

本节更新前文“尚未接入上传服务”的状态；管理员 UI 与跨语言/UAT 场景仍待完成。

- 既有管理员 revision multipart 接口新增显式 `format=teamai-candidate-v1`，候选模式不接受 `git_commit`。普通文件上传继续使用原流程。
- `UploadCandidate` 先验证管理员身份、范围及资产状态，再解析候选并复用现有内容存储/创建 revision；不调用发布入口。
- 新增 `0266_asset_revision_source.sql`，保存声明的转换来源；读取返回 source，同内容复用原 revision 时保留第一次来源，不用新候选改写已有元信息。普通 revision 的 source 为 SQL NULL。
- 原始候选 archive_sha256 记录于来源；服务端重打包后的 archive_sha256 继续由服务器计算，二者不混用。
- OpenAPI 已同步，schema.d.ts 通过生成器更新；`assets:check` 通过。

实际检查：资产全部包、handler、persistence 测试通过，其中 persistence 使用已存在的隔离 PostgreSQL 容器和测试数据库，实际执行迁移与来源读回/重复导入测试；完整 `go test ./...`、Gateway build、`go vet ./...` 均通过。新增 multipart HTTP 测试随后单独通过，覆盖管理员成功、普通成员/成员 Key 拒绝、未知格式及混用 git_commit 拒绝。

迁移清单检查首次因新增 ledger 行 CRLF 失败，修正 LF 后 `verify-migration-contracts.sh` 通过，当前工作区迁移头为 0266。GitNexus impact/context 对相关符号返回未找到，detect-changes 对工作树仍返回 Repository not found；图覆盖缺口未解决。本节没有声称 CI、UAT、完整 UI 或跨语言导入已验收。

## 后续检查点：管理员导入表单（2026-09-11）

- AssetUploadForm 识别 `.nexusops-asset.json`，先展示候选名称、类型、声明的转换来源、内容哈希和 Skill 文件数。候选预览只是本地结构检查，最终内容由服务器验证。
- 明确确认后发送原文件及 `format=teamai-candidate-v1`；不发送 git_commit，不自动发布。错误或类型不匹配时不会退回普通 JSON 上传。
- 文件重选、资产切换、组件卸载使旧预览失效；上传结果不能污染已切换的资产。普通上传仍保留原有入口。
- revision 只读预览新增来源展示。中英文资源已补齐，其余管理控制台语言沿用既有 fallback。
- 资产模块 6 个测试文件、37 项通过；typecheck、lint:check、生产 build 通过。测试首次有新增文件语法错误，修复后重跑通过；不是整个管理控制台所有测试已运行的声明。
- 实际 Vue 组件的独立浏览器夹具验证了文件选择、预览、确认前禁用、确认后上传和成功回调；只调用一次候选上传（asset ID 17），无页面错误、无横向溢出。截图 `admin-candidate-preview.png` 明确标记合成候选与上传替身；首次夹具因 Vue runtime-only 不支持模板编译未渲染，改用 render 函数后在新会话验证，不抹去首次失败。
- GitNexus 仍无工作树图覆盖，使用源码、类型检查、组件测试及浏览器检查补足。下一步仍需真实客户端导出 → Go 导入、审批发布和成员安装的跨语言闭环；T13 未验收完成。
