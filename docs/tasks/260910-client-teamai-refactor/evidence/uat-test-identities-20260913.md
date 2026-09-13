# UAT 测试身份创建与验证

2026-09-13，用户明确要求执行者生成并处理 UAT 测试管理员 / 成员账号。此次只新增专用 QA 身份及测试分组，没有替换既有账号凭据。Gateway 仍为 23a5657416db57938b78cf2c86a4533153a8220e，未重新部署或切换 release 模式。

## 已创建

| 身份 | 用户 ID | 角色 | 成员 Key ID |
| --- | --- | --- | --- |
| TeamAI QA 管理员 | 7 | admin | 不使用成员 Key |
| TeamAI QA 成员 A | 8 | user | 24 |
| TeamAI QA 成员 B | 9 | user | 25 |

邮箱均使用带随机标识的 qa-client-teamai-20260913-* @example.test 保留测试域。独立 QA 分组 ID 11，仅分配给上述两个成员。成员余额为 0，Key 设 7 天有效期、0.01 配额及相应周期上限；控制面访问不依赖模型余额。

## 创建路径与约束

UAT SERVER_MODE=release，初始化 ADMIN_PASSWORD 未保留，开发专用账号命令不可用。没有覆盖运行模式或伪造登录令牌。

为完成用户明确授权的初始测试管理员创建，使用一次性 SSH 运维程序调用当前版本的 NewManagedUserRepository / User.SetPassword：重新验证签名工作区授权，复用席位检查、共享数据库锁、邮箱唯一性及 email auth identity 创建。创建前 6/25 席位，足够新增三人。新管理员及运维审计在同一事务内提交，只允许本次 QA 邮箱前缀且已有账号即拒绝，不提供重置 / 更新动作。

一次性程序 SHA256 为 673d09702ae06f6584da41324571a9600ed8b809999e58044abe776d25769d4d。程序已从 UAT 容器、远端临时目录和本地临时构建位置移除；源码留在本机私有交付目录供操作审计，没有加入服务端发行程序。初次终端管道带来的 JSON 输入失败发生在写入之前，改为 UTF-8 字节 stdin 后正常执行。

测试管理员随后走正常密码登录，两个成员通过正常管理 API 创建。为满足成员 Key 签发的强制 step-up，按正常密码验证流程给这个新管理员启用 TOTP，实测密码登录要求二次验证、TOTP 登录成功、step-up 成功，再调用管理 API 签发 Key。没有关闭 MFA、Turnstile、席位或其他权限检查。

GitNexus context 查询因服务端工作树未注册失败，已明确记录覆盖缺口；本次通过源码、编译、只读席位检查、事务和实际 API 验证核对运维范围，没有修改产品代码。

## 实测结果

- 管理员与两个成员的正常登录均 200；管理员角色 admin，成员角色 user。
- 两个成员 JWT 访问管理员用户接口均 403。
- 两个成员 Key 访问 /api/v1/me/teamai/projects 均 200；分别返回 member_id=8 / 9，组织与工作区均匹配当前 UAT。
- 成员 Key 读取管理员工具统计均 401；管理员会话读取该接口 200。
- 首次成员 Key 签发因没有分组返回 400，未算成功；建立独立 QA 分组并授权两个成员后，签发和作用域验证通过。
- Team Profile 初次返回 503，原因 ASSET_PROFILE_BASE_URL_REQUIRED。通过管理员设置 API 仅提交 api_base_url=https://uat-api.ai-nexusops.app/v1；前后读取确认仅此设置变化，随后两个 Profile 导出均 200。
- Profile 的 gateway_url、workspace、organization、key ID 均已核对。当前测试分组未配置模型上游，实际 models 列表为空；身份、资产和上报接口可测试，模型调用尚未配置。
- 后续再次读取 QA 用户列表，准确得到 ID 7、8、9；UAT health 为 ok，revision 未变。

结构化脱敏结果见 uat-test-identities-20260913.json。密码、TOTP 种子、访问令牌、完整成员 Key 和 Profile 存于本机 Git 目录之外的受限目录，访问权限已核对仅授予当前用户；交付消息提供路径。凭据不进入本仓库、公开验收日志或聊天正文。

## 对原任务的影响

“缺少 UAT 测试身份”阻塞已经解除。下一步可使用这组身份完成候选审批、两工具同步、工具事件上报和管理员对应成员对账。此次不等同这些完整业务链路已通过；托盘人工结果及干净 Windows 环境验收仍未完成。
