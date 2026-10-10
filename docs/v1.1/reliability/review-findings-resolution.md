# v1.1-r2 复审问题修复记录

日期：2026-10-10。分支：公共与私有仓库均为 `fix/v1-1-review-findings`。

本文件记录《交付/审查/2026-10-10/审查报告.md》中 12 项发现的修复结果。它不是正式发布批准；备份恢复演练、安装包生命周期和实际部署版本仍保持 `NOT_TESTED`。

| 发现 | 修复结果 | 主要实现 | 验证 |
|---|---|---|---|
| F01 验收脚本虚假 PASS | 修复 | runner 逐项执行实际命令，拒绝零测试；退出码 1=FAIL、2=NOT_TESTED | 无效 PostgreSQL 地址得到 FAIL；隔离 PostgreSQL 得到 E01—E19 PASS、E20—E22 NOT_TESTED |
| F02 登录复用业务库 | 修复 | 登录后按可信 account.id 创建独立 Profile DB 和 scoped SecretRef，使旧租约失效并重启桌面；退出切回游客 Profile | `login_route_activates_a_separate_database_for_each_account` |
| F03 缺失 sync epoch 握手 | 修复 | 首次同步调用 capabilities，校验协议并绑定 epoch；epoch 变化触发恢复与 cursor 重置 | `first_sync_negotiates_and_persists_epoch`、真实 PostgreSQL协议门禁 |
| F04 in_flight 不恢复 | 修复 | 网络、HTTP、解析失败和 partial ACK 均恢复到 retry_wait；批次幂等键由 change_id 集合稳定生成 | `failed_push_is_retried_with_the_same_idempotency_key` |
| F05 下载内容被重新采集 | 修复 | 云版本 digest 与业务 payload fingerprint 分开；采集与投影使用同一规范化 fingerprint | `unchanged_download_is_not_recaptured_as_a_local_edit` |
| F06 隐私开关仅界面生效 | 修复 | 新增 Profile 内 `cloud_sync_policy`、GET/PUT API；真实采集读取保存策略；失败时 UI 回滚 | 前端映射测试、浏览器开关与后端持久化检查 |
| F07 白名单不完整 | 修复 | 客户端所有同步 DTO 校验 schema；嵌套结构限制深度/大小并拒绝敏感键；云端验证实体外层字段、schema、message role 与设置 section | PUBLIC payload tests、PRIVATE review regressions |
| F08 capability_usage 无法恢复 | 修复 | 客户端增加幂等投影和删除；云端同事务写入 capability_usage 独立投影 | `advertised_capability_usage_is_projected_on_pull` |
| F09 注销 SQL 列错误 | 修复 | 按实际 `owner_scope` 清理 idempotency_records | 真实 PostgreSQL `test_review_deletion_idempotency_cleanup_and_backup_receipt_gate` |
| F10 等待状态不再调度 | 修复 | waiting_for_producers 在写者排空后可领取；waiting_backup_expiry 在存在清理回执后可领取并生成新 lease | 同一真实 PostgreSQL 生命周期测试 |
| F11 未核验备份却完成 | 修复 | 新增 backup_purge_receipts；无回执一律保持 95% 等待，SystemAdmin 可提交带审计的回执 | 真实 PostgreSQL等待→回执→重领→完成测试 |
| F12 下载绕过认证 | 修复 | 使用 API_BASE + control-session header 获取 Blob，再触发本地下载 | `requestBlob` 测试；前端构建通过 |

同时完成四项复审追加要求：

- ScopedDb 维护共享活动租约；Profile 切换取得写门闩并使所有旧租约立即失效。取消任务时不再提前从活动表移除，而是等待真实执行路径退出。
- 旧 SQLite 在创建 Profile 前使用 Backup API 生成隔离备份，并把一致数据副本作为活动游客 Profile 导入；源库继续保留，避免升级后出现空库。
- 2016 本地迁移记录一次性可靠性 reconciliation；启动时真实执行，保留 outbox 并重置错误 cursor。
- 启动时调度 pending remote revocation 重试；普通请求只读取当前 Profile 记录的 scoped SecretRef。

## 验收边界

本轮真实 PostgreSQL 容器绑定 `127.0.0.1:55432`，数据库为 `yilian_review_test`。验证结束后容器已删除，Docker Desktop 已正常停止。临时数据库只包含合成测试数据。

修复后 runner 的隔离数据库结果：E01—E19 PASS；E20、E21、E22 为 NOT_TESTED，进程退出码 2。E20 需要可核查的备份文件、独立恢复目标与删除账本重放；E21 需要实际安装包首装/升级/卸载；E22 需要实际 API/worker/admin 运行版本。缺少这三类外部证据时禁止改为 PASS。

浏览器插件在本会话不可用，按前端测试技能回退到本机 Playwright + Chrome。实际页面验证路径为 `/setup` 跳过本轮测试向导 → `/account?section=privacy` → 切换“对话与消息” → 读取 `/api/cloud/sync/policy` 确认后端值改变。1440×960 和 820×900 两种视口均无布局阻断，控制台无相关错误。

## 发布结论

代码修复状态为 `IMPLEMENTED`。正式发布状态仍为 `BLOCKED_BY_EXTERNAL_ACCEPTANCE`；在 E20—E22 和新的 S8 独立复审完成前，不开放生产自动云同步和永久注销。
