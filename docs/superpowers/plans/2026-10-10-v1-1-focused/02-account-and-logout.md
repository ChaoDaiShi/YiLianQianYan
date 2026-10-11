# P2：账号切换必须登录，退出结果必须真实

> 待实施。对应 R01、R05；本包内部账号入口与退出一起验收，避免两组人员同时修改 profile/session。使用 executing-plans；未经用户明确告知，不使用子智能体。

## 文件范围

以下均相对 CLIENT：

- 实现：`backend/src/api/cloud.rs` 中认证/切换/退出函数；`backend/src/modules/cloud/profile.rs`、`session.rs`、`revocation.rs`；`backend/src/app/bootstrap.rs` 的待撤销处理。
- UI：`frontend/src/pages/AccountCenterPage.tsx`、`frontend/src/features/cloud/AccountProfilePanel.tsx`、`profileSwitch.ts`、`frontend/src/api/cloud.ts` 的退出类型。
- 测试：扩展 `backend/tests/v11_profile_isolation.rs`；新增 `backend/tests/v11_account_logout.rs`；扩展 `frontend/src/features/cloud/profileSwitch.test.ts`，新增 `frontend/src/pages/AccountCenterPage.test.tsx`（若实施时已存在，则扩展）。
- 只复用 P1 的 HTTP helper；不在本包修改 URL 规则。与 P3 共用 `AppServer.cloud_operation_lock`，字段及初始化由后续实施的 P0 统一交付。

## 2A. 最小账号切换流程

直接输入 account_id 的入口没有必要继续保留。采用“切换账号 → 现有登录表单 → 云端 login → 服务端 account_id → 现有 prepare/persist/activate → Tauri 重启”的单一流程。

```rust
// 替换原 profile_switch_handler 的函数体，保留旧路由和请求类型。
pub async fn profile_switch_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_payload): Json<ProfileSwitchPayload>,
) -> ApiResult {
    Err(api_error(
        StatusCode::UNAUTHORIZED,
        "profile_authentication_required",
        "请登录目标账号后切换资料库",
    ))
}
```

`authenticate` 继续从 `AuthenticatedSessionInput.account_id` 构造 ScopeKey，复用 `persist_authenticated_session_scoped`。不能用请求 account_id 覆盖响应 account_id，也不能先激活目标 DB 再认证。

前端移除 service origin + account_id 的手动切换表单，用 `onRequestLogin: () => void` 回调打开 `AccountCenterPage` 已有登录区；沿用已有 `loginCloud` 和 `restart_application`。失败时不清空当前账号的页面数据。资料库面板保留必要状态，不新增 PIN、本地密码、额外密钥或新的账号选择引擎。

## 2B. 显式退出标记与启动行为

只删除凭据不能表示“用户明确要求退出”，因为其中某个步骤可能失败。利用已有 profile-registry.json 给 ProfileEntry 增加一个字段：

```rust
#[serde(default)]
pub reauthentication_required: bool,
```

配套唯一辅助函数：

```rust
// 新增于 ProfileManager；按现有 load_or_initialize/save_registry 写同一个注册表。
pub fn set_reauthentication_required(
    app_data_dir: &std::path::Path,
    profile_id: &str,
    required: bool,
) -> Result<(), crate::modules::cloud::contracts::ScopeError>;
```

这是单字段持久化，函数签名是待实现契约，不是另一套解锁系统。

规则：退出开始时先对当前账号标记 true；清理失败也不清除。`resolve_active_profile_db_path` 在打开带此标记的账号前改为访客激活；只能在云端密码认证成功、scoped session 成功保存后将对应标记设为 false。`prepare_profile` 重写已有 ProfileEntry 时保留其标记，不能靠新结构体默认值提前解除。网络暂不可用或 token 过期不自动设置此字段，保持未退出账号的现有离线使用能力。

兼容旧注册表：新字段缺省 false；历史账号 DB 若是 `offline_authenticated` 且 `current_session_id=NULL`，按既有退出路径识别为已退出，首次读取时改为需要登录。必须先核对该状态组合在现有调用点仅由显式退出产生；若发现别的实际写入者，针对该路径保留离线语义，不扩大为“所有离线账号必须联网登录”。

威胁范围是应用内切换误开放本地资料；不宣称能够阻止拥有同一 Windows 用户文件权限的人直接读取 SQLite 文件。

## 2C. LogoutReport 和本地清理

在原 `revocation.rs` 原位扩展类型，旧字段保留：

```rust
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCleanup { Cleared, Failed }

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRevocation { Confirmed, Pending, Unknown, NotRequired }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LogoutReport {
    pub local_signed_out: bool,
    pub local_cleanup: LocalCleanup,
    pub remote_revocation: RemoteRevocation,
    pub error_codes: Vec<String>,
}
```

保持 `RevocationService::logout` 的参数列表；删除 `let _ = ...` 及 `.ok().flatten()` 隐藏清理错误的路径。普通凭据删除要逐项尝试，不能第一个失败就跳过剩余项：

```rust
// 放在 logout 原函数内，lease/self 与现有代码一致。
let mut error_codes = Vec::<String>::new();
if self.db.mark_cloud_profile_offline().is_err() {
    error_codes.push("LOCAL_IDENTITY_WRITE_FAILED".into());
}
for secret_ref in [
    scoped_cloud_access_token_ref(&lease.scope_id),
    scoped_cloud_refresh_token_ref(&lease.scope_id),
    cloud_access_token_ref(),
    cloud_refresh_token_ref(),
] {
    if self.secret_store.delete(&secret_ref).await.is_err() {
        error_codes.push("LOCAL_SECRET_DELETE_FAILED".into());
    }
}
error_codes.sort();
error_codes.dedup();
let local_cleanup = if error_codes.is_empty() {
    LocalCleanup::Cleared
} else {
    LocalCleanup::Failed
};
```

执行顺序固定：

1. handler 取得云操作锁，记录当前 scope/session；为当前账号持久化需要重新登录的标记。
2. 从 SecretStore 读取远端撤销所需 token。读取失败作为远端 unknown，继续尝试清理其他本地项；不阻断用户退出。
3. 本地账号设为离线，逐项删除普通 access/refresh（含遗留引用）。保留当前内存中远端请求使用的 token，不能再从已删除位置取。
4. 尝试远端撤销；失败时按现有队列保存待办和 token 引用。此 token 仍是原 access token，**不能把“放到撤销专用引用”表述为“权限已经收窄的 token”**。本轮不新增服务端撤销专用令牌协议。
5. 尝试访客准备及激活；失败记 `GUEST_ACTIVATION_FAILED`。最终 `local_signed_out` 只在本地全部步骤成功时为 true。
6. 本地失败 HTTP 500 + C02 错误；成功 HTTP 200 + activation/report。重启后访客页面仍能显示远端 pending/unknown，不把它们描述成已撤销。

注册表标记持久化失败时返回错误且不声称退出。已设置标记后续清理失败时，界面提示重试；重启按标记进入访客。不要通过清空历史对话或删除账号数据库解决失败。

前端 `ProfileSwitchController.logoutWithReport` 改为 `Promise<ApiResult<CloudLogoutResponse>>`，直接透传 `logoutCloud()` 的结果，只在 `ok=true` 时读取报告。删除出错后构造 `local_signed_out:true` 的 fallback。状态展示根据本地结果和远端状态分别生成文本，不让 `remote_revocation=confirmed` 隐含本地清理成功。

## 2D. 重试终态和退出后可达性

沿用 `cloud_pending_revocations` 表，沿用五次自动重试上限。失败到上限时更新 `last_error_code='RETRY_EXHAUSTED'`，保留行，列表界面显示 unknown。缺 token 为 `REVOKE_CREDENTIAL_MISSING`，保留行；SecretStore 暂时读取失败与“确实不存在”必须区分。

```sql
-- 在原 list_pending_revocations 查询中约束自动领取，终态仍可查询。
WHERE attempts < 5
  AND next_retry_at <= ?1
  AND COALESCE(last_error_code, '') NOT IN
      ('RETRY_EXHAUSTED', 'REVOKE_CREDENTIAL_MISSING')
ORDER BY next_retry_at ASC LIMIT 50
```

远端确认成功后才能删除待办；401/网络错误不算 confirmed。404 只有服务端所有权检查后明确“该会话不存在”的契约才可当作确认；不能把无关网关 404 当成功。达到上限可以删除保存在 SecretStore 的待撤销 token，删除错误须保留代码，不删除审计结果。

当前启动仅对 active DB 执行重试，退出重启后 active 是 guest。新增局部入口：

```rust
pub async fn retry_registered_profile_revocations(
    app_data_dir: &std::path::Path,
    secret_store: std::sync::Arc<dyn crate::integrations::secret::SecretStore>,
) -> Result<usize, super::session::CloudSessionError>;
```

读取现有 registry 中账号 DB 路径，逐个打开已有 DB、只读取/更新待撤销表并调用原 `retry_pending_revocations`；不执行资料库激活，不返回对话，不写 session。缺失数据库跳过并记录固定错误；不为失踪 profile 创建空数据库。启动处用本入口替代“仅 active DB”的调用。终态摘要可由已有 `profile_handler` 附加 `{session_id,remote_revocation,last_error_code}` 提供，不提供 token。

## 验收与交接

| 用例 | 操作 | 判定 |
|---|---|---|
| P2-01 | A 登录并创建内容 → 退出 → 直接向旧 switch 发 A 的 origin/account_id | HTTP 401；registry/lease 未变；对话与记忆不可在应用内读取 |
| P2-02 | 登录 B，试图指定 A | 激活 B；目标只来自云端响应 |
| P2-03 | A 正确密码再次登录 | 恢复 A 原目录和原数据，不复制到新 guest |
| P2-04 | 注入 DB 写失败、逐个 SecretStore delete 失败 | HTTP 500，无本地退出成功提示，其余凭据仍尝试清除 |
| P2-05 | 远端离线 → 退出 → 重启为 guest → 远端恢复 | 待办仍可重试；不激活旧账号；确认后才移除待办 |
| P2-06 | 重试五次失败/凭据缺失 | 终态记录保留，显示 unknown，不显示 confirmed |
| P2-07 | 普通断网但未退出，重启 | 原离线资料可用；不把断网当退出 |
| P2-08 | 清理失败后强制重启 | 退出标记生效，应用进入访客，不能手动打开旧账号 |

测试注入直接复用现有 SecretStore trait 的测试实现及临时 SQLite；只模拟具体 put/get/delete 错误，不新增生产故障注入框架。

```powershell
cargo test -p yilian-backend --test v11_account_logout --test v11_profile_isolation
cargo fmt
cargo fmt --check
cargo check --workspace
cd frontend
npm.cmd test -- --run src/features/cloud/profileSwitch.test.ts src/pages/AccountCenterPage.test.tsx
npm.cmd run build
```

- [ ] 先让 P2-01、P2-04 在基线失败。
- [ ] 合并 2A、2B、2C、2D 的小补丁，每次运行相关用例。
- [ ] 独立验证通过后向 P7 交付两账号真实桌面复验步骤及 commit。

不实施注销二次认证、数据库加密或 Windows 用户隔离；这些不是本包闭环所需功能。
