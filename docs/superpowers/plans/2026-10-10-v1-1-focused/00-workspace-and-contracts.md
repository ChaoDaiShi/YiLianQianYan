# P0：独立工作区与接口约定

> 状态：目录拆分已完成；公共云操作锁字段与初始化已合入并通过 workspace 验证；业务接口契约已锁定供 P1—P6 开发。执行采用 executing-plans；未经用户明确告知，不使用子智能体。最高约束：严禁防御性开发。

## 实际目录和拆分证据

```text
F:\项目开发\忆涟千言\
  YiLianQianYan\        桌面本体；独立打开
  YiLianCloud-Private\  云服务、workers、管理后台；独立打开
  YiLian-v2\            原冻结版本
```

旧服务器位置为 `YiLianQianYan\.private\YiLianCloud-Private`，现已整体迁到同级目录。SERVER 使用自己的 `.git`，移动前只有一个工作区；没有 Git worktree 指针需要修复。迁移时 HEAD=`fc867bf4`、tree、所有 refs 和 87 个非缓存文件哈希一致。缓存及 `.git` 随目录整体移动，未做内容清理。

本机证据位于 CLIENT 的 `交付/修复验证/2026-10-10/server-directory-split/before.json` 和 `verified.json`。这些记录只留本机，不应上传公共仓库。

公共 Cargo workspace 不包含服务端。已有跨仓库验证器使用显式参数；执行时使用新路径：

```powershell
pwsh -NoProfile -File .\scripts\verify-v11-reliability.ps1 `
  -PublicRoot 'F:\项目开发\忆涟千言\YiLianQianYan' `
  -PrivateRoot 'F:\项目开发\忆涟千言\YiLianCloud-Private' `
  -EvidenceRoot 'F:\项目开发\忆涟千言\YiLianQianYan\交付\修复验证\candidate'
```

不要修改历史报告中的已发生路径；旧计划入口增加现行路径提示即可。SERVER 的 README 和本地项目入口使用新地址。服务端运行的数据库 URL、对象存储绝对路径不因源码移动而自行改写；有旧绝对路径的本地启动配置只在实际使用前定位修正，不能打印配置秘密。

## 契约 C01：账号切换复用登录

继续使用 `POST /api/cloud/login`：

```json
{"cloud_base_url":"https://cloud.example.com/api/v1","email":"a@example.com","password":"<运行时用户输入>"}
```

成功仍返回现有 `{"activation": ActivationPlan, "status":"awaiting_restart"}`；ActivationPlan 使用现有 `profile.rs`/`frontend/src/api/cloud.ts` 定义，不再引入第二种重启协议。目标 account_id 必须来自云端登录响应。

原 `POST /api/cloud/profile/switch` 保留路由，停止按客户端提交的 account_id 激活。返回：

```json
{"code":"profile_authentication_required","error":"请登录目标账号后切换资料库"}
```

HTTP 401，无文件创建、无租约改变、无运行期取消副作用。前端“切换账号”打开已有登录表单；测试保留直接调用旧路由的路径，不能只删 UI。

## 契约 C02：退出结果

沿用 `POST /api/cloud/logout`。成功响应：

```json
{
  "status":"awaiting_restart",
  "activation":{"profile_id":"guest-example","lease":{"scope_id":"guest-example","generation":"example"},"requires_restart":true,"import_count":0},
  "report":{
    "local_signed_out":true,
    "local_cleanup":"cleared",
    "remote_revocation":"pending",
    "error_codes":[]
  }
}
```

示例 activation 保持已有 ActivationPlan 的完整形状。`local_cleanup` 只有 `cleared|failed`；`remote_revocation` 采用 `confirmed|pending|unknown|not_required`。`not_required` 仅用于已确认的访客/无远端会话，不能把账号会话信息缺失当作 confirmed。

本地清理失败返回 HTTP 500：

```json
{"code":"local_logout_incomplete","error":"本地退出未完成，请重试","report":{"local_signed_out":false,"local_cleanup":"failed","remote_revocation":"unknown","error_codes":["LOCAL_SECRET_DELETE_FAILED"]}}
```

前端 `requestResult` 失败分支只需要显示 error，不得构造成功 LogoutReport。`local_signed_out=true` 要求数据库身份失效、普通 access/refresh 凭据清理、访客激活持久化均成功。远端 pending 或 unknown 不妨碍本地退出，但要显示“远端撤销未确认”。P2 完整定义执行顺序和终态保留。

## 契约 C03：同步开关

现有 `GET/PUT /api/cloud/sync/policy` 和十个布尔字段不改名。关闭表示该类别不采集、不出队上传、不把远端记录写入业务表、mirror、冲突正文或指纹表。已有本地数据、云端数据均保留。

组内必须一致：`sync_conversations=sync_messages`；`sync_task_graphs=sync_canvas_views`。不一致返回 HTTP 400 `sync_policy_group_mismatch`。UI 已按这两组保存，无需新增复杂选项。

同步正在执行时，策略修改返回 HTTP 409：

```json
{"code":"cloud_operation_in_progress","error":"当前正在同步，请完成后重试"}
```

使用同一个 `AppServer.cloud_operation_lock: tokio::sync::Mutex<()>`，保护同步、策略保存、登录激活和退出。采用 `try_lock`，不引入后台队列；HTTP 409 时不改变本地开关显示。该锁仅解决真实的在途同步/切换交叉，不替换现有 ScopeLease 和 SQLite 事务。

已在 P0 先行合入：在 `backend/src/app/state.rs::AppServer` 加入此字段，在 `backend/src/app/bootstrap.rs` 的 AppServer 构造中使用 `tokio::sync::Mutex::new(())` 初始化，并通过 `cargo check --workspace` 及运行时测试。P2/P3 后续可直接从当前基线开发各自的调用逻辑。P0 不提前修改业务处理代码。

重新开启任一类别时，在策略保存的同一 SQLite 事务中把下载检查点重置为 `"0"`，保留 epoch、outbox、mirror、实体版本及冲突。下一次采集先登记本地变化，再执行原有同步；版本判定保护已同步数据，待上传变化进入原冲突流程。不得调用会额外合成冲突的 `reconcile_skipped_history` 来实现这个简单重放。

## 契约 C04：数据 API 错误

对话与记忆成功响应保持现有对象/数组形状。只有失败响应改为非 2xx：

```json
{"code":"local_storage_unavailable","error":"本地数据暂时无法读取，请重试"}
```

数据库失败：500。明确查无对象：404；仅用类型/Optional 结果区分，不匹配底层 SQL 错误字符串。查询成功且无记录仍返回 200 `[]`。已经持久化的记忆，附加 embedding 失败不撤回创建结果，保持现有能力。

## 交接格式

各阶段交付一份记录，字段固定，不包含密码、令牌、聊天正文、数据库连接串：

```json
{
  "phase":"P1",
  "status":"PASS",
  "repository":"CLIENT",
  "commit":"实际提交号",
  "cases":[{"id":"P1-01","status":"PASS","evidence":"P1-01.log"}],
  "shared_file_patches":["backend/src/api/cloud.rs::cloud_client_error"],
  "remaining":[]
}
```

`PASS`、`FAIL`、`NOT_TESTED` 为单项验收状态；工作包未实现时为 `NOT_STARTED`，不得提前生成 PASS 文件。P0 只约定契约；不为了并行提前建空实现、假接口或占位 SQL。
