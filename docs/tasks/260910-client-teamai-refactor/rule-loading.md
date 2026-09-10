# Rule 加载依据与接入约束

核对日期：2026-09-11。本机 Codex CLI 为 0.153.2、Claude Code 为 2.1.263。原生适配、真实文件安装 / 恢复及两种 CLI 的实际加载已通过隔离测试；正式发布 / UAT 联合验收尚未完成。

| 工具 | 官方加载方式 | 本项目接入要求 |
| --- | --- | --- |
| Codex | 全局指令优先读取非空 AGENTS.override.md，否则读取 AGENTS.md；指令总量默认受 32 KiB 限制 | 使用实际生效指令文件内的 NexusOps 管理区块，保留区块外用户内容；检测 override 和容量，不能写进被忽略的文件 |
| Claude Code | 用户级 ~/.claude/rules/ 中的 Markdown 规则会加载，支持 paths 条件 | 将规范接入工具规则目录，保留条件；路径使用已有配置目录解析器，不能写死用户主目录 |

依据：[Codex 指令发现规则](https://learn.chatgpt.com/docs/agent-configuration/agents-md)、[Claude Code 规则与条件](https://code.claude.com/docs/en/memory)。

Codex 的 `.rules` 文件用于沙箱外命令执行策略，采用 Starlark；不能把 TeamAI Markdown 规范复制为 `.rules` 或自动转换为命令放行策略。[官方 Rules 文档](https://learn.chatgpt.com/docs/agent-configuration/rules)

实现与后续验收继续遵循原 task.md：

- 保留规范化资源副本及其哈希；工具入口变更归同一个 Rust 执行器管理。
- 预览展示实际加载入口；保留个人内容，检测管理标记损坏、手工编辑、路径条件和容量限制。
- 写入前核对本机状态，保留备份与恢复信息；工具入口未达到期望状态时不标记安装完成、不发送成功 ACK。
- 兼容既有 managed download 的本机记录，不因升级丢失恢复路径。
- 使用两种 CLI 的实际加载证据验收，不能以目录存在或文件下载成功替代。

实测补充：Codex 0.153.2 的 BOM-only override 会遮蔽 AGENTS.md；实现已与该行为一致。CLI 加载测试先调用固定 TeamAI worker 转换带 frontmatter 的 Rule，再写入原生入口并启动真实 CLI。测试只在本机捕获服务中检查特定标记，不保存完整提示词，也不调用真实模型；因此这不是 UAT 或模型遵从性证明。Claude paths 原文保留，Codex 中则作为规范文本呈现，不冒充 Claude 的原生按路径加载。
