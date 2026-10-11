# 忆涟千言 v1.1 复审问题修复记录

> 日期：2026-10-11
>
> CLIENT 基线：`docs/v1-1-final-review@1ebdb69ff9dca92214e4c204a777f1f6c20a6f4b`
>
> SERVER 基线：`test/v1-1-server-acceptance@95f2adcb3fc802c3627fab1b24fe5b5021727e1f`
>
> 状态：代码修复与自动化验证完成；真实两设备和发布环境验收仍未完成。

## 修复结果

| 复审问题 | 修复结果 | 关键实现 |
|---|---|---|
| 延迟 refresh 在退出后恢复凭据 | 公开刷新入口与退出、登录、同步共用 `cloud_operation_lock`；在途刷新时退出返回 409，完成后再退出 | `backend/src/api/cloud.rs`、`v11_account_logout.rs` |
| 清理失败后重试清错访客 scope | 本地清理失败时不切换运行期 lease；重复退出继续处理原账号；重启进入访客后，后台循环仍按注册表中的原账号 scope 清理残留凭据 | `cloud.rs`、`revocation.rs` |
| 待撤销任务只在启动时尝试一次 | 保留启动时立即尝试，并增加 30 秒周期的可取消 Tokio 循环；测试用短间隔验证无需再次重启 | `bootstrap.rs`、`revocation.rs` |
| 访客无法看到远端撤销状态 | 汇总所有已注册账号数据库的脱敏待办；访客账号页显示 pending/unknown，不显示账号、会话或凭据内容 | `revocation.rs`、`AccountProfilePanel.tsx` |
| 记忆保存失败丢失编辑草稿 | store 返回保存结果；页面只在成功时关闭弹窗。浏览器复验确认 HTTP 500 后弹窗和草稿保留 | `memoryStore.ts`、`MemoryPage.tsx` |
| 云/Profile 文件超出 600 行 | 错误映射和启动解析分别拆到专责模块；原 API 和行为保持不变 | `cloud_errors.rs`、`profile_resolution.rs` |
| E20 删除清单查询失败仍继续 | 使用真实 users schema；psql 非零、无效 JSON、空清单均失败；不再把查询失败写成 `[]` | SERVER `verify-backup-restore.ps1` |
| E20 零处理量被当作完成 | 恢复重放跟踪本次 job IDs，核对 status/live_data_removed、会话、实体、对象与文件；无进展或失败直接退出非零 | SERVER `replay_restored_deletions.rs` |
| PostgreSQL 恢复测试夹具字段错误 | 按真实 users/devices/sessions/sync_entities 字段及 UUID/外键重写 | SERVER `backup_restore_postgres.rs` |

## 本次验证

| 检查 | 结果 |
|---|---|
| `cargo fmt --check`（CLIENT） | PASS |
| `cargo test -j 1 --workspace --all-targets --no-fail-fast -- --test-threads=1` | PASS：1129 passed，0 failed |
| `npm.cmd test -- --run --maxWorkers=1 --minWorkers=1` | PASS：113 files，542 tests |
| `npm.cmd run build` | PASS；保留既有大 chunk 警告 |
| 记忆编辑失败浏览器组件复验 | PASS：PUT 500 后 modal=1，草稿内容不变 |
| 访客撤销摘要浏览器复验 | PASS：pending 可见，账号 profile_id 与 session_id 不出现在页面 |
| `cargo fmt --check && cargo check --workspace && cargo test --workspace --all-targets`（SERVER） | PASS：标准套件 20 passed，14 个 live DB 用例按设计 ignored |
| 隔离 PostgreSQL 16 live suite | PASS：14/14；临时容器已删除 |
| E20 清单查询失败负对照 | PASS：错误向上返回，验收器不再输出 PASS |

浏览器复验使用项目已有 Playwright 和本机 Chrome；本会话没有 Browser 插件。页面使用合成 API，不访问生产账号。截图及结果保存在本机复验证据目录，不提交仓库。

## 尚未关闭的发布验收

- D01—D05：仍需两台独立 Windows 设备或虚拟机执行真实账号、同步和故障流程。
- E20：恢复代码、真实 PostgreSQL 用例及失败负对照通过；完整的旧备份 `pg_dump/pg_restore`、T0 后注销和对象存储恢复演练仍未在本环境执行。
- E21：未执行干净机安装、跨版本升级、卸载和重装。
- E22：未核对实际运行服务的制品或镜像摘要。

因此当前可判定“复审发现的代码缺陷已修复”，不能判定 `READY_FOR_RELEASE`。发布结论仍为 `ACCEPTANCE_INCOMPLETE`，直到真实环境证据完成。
