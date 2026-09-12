# 原有个人功能的源码保留复核

2026-09-12。基线 f811e352fe3904214a2b2305baff81964a416c56，当前源码 ce86a64e55f39011b823e9b8ad2a5c45bfc0d39c。此项为 T01 / T10 的源码证据，不等于所有外部工具的实机操作均已通过。

对 prompts、mcp、skills、agents、providers、sessions、workspace、openclaw、hermes、settings、usage 共 11 个 src/components 子目录执行 Git 树比较：136 个基线文件无删除或改动。对 src/hooks、src/lib/api、src/lib/query、src/config 的同范围 diff 也为空。新的 Team 与 shell 实现在其他目录，不据此宣称全项目未改。

使用 TypeScript AST 定位基线与当前 App 的 handle 前缀回调，去注释规范打印初始化表达式后比较：14 个原回调均存在，13 个表达式完全一致，包括 Provider 编辑、确认删除、复制、启用 Pi、禁用 OMO、导入结果、终端/网站入口、窗口最小化/最大化/关闭和 Skills 发现入口。

唯一变化是 handleKeyDown：新首页不再响应 Escape 返回；其他页面改用 parentViewFor，Skills 发现仍回 Skills，其他页面按新的业务父入口返回。Ctrl+,、管理中阻塞、文本编辑目标保护及 modal overflow 保护未变。实际新包快捷键与回退证据见 [2fa03d89 原生回归](package-and-native-2fa03d89.md)。

结构化比较见 `legacy-preservation-source-20260912.json`。这避免把未改动的旧编辑器重新算作新增实现，并与真实 15 类保存值重启、Prompt 完整文件操作、新旧用量数据对照及既有测试共同评估兼容性。

局限：比较的是这些目录和具名回调，未覆盖所有匿名函数、依赖或后台运行路径；相同源码不等于一切运行条件相同。托盘菜单操作、身份与权限、真实 UAT 资产 / 统计联调仍按原任务单独验收，不以本结果替代。
