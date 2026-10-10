# S6 数据最小化、插件使用闭环与授权拒绝

> 状态：COMPLETED。本阶段同时向 S1 提供服务端验证器、向 S2 提供本地采集器。旧授权解析修复已合入，数据同步按 S0 契约与严格白名单集成。

## 1. 文件

- PUBLIC 新增 `backend/src/modules/cloud/payload.rs`、`backend/src/db/cloud/usage.rs`。
- PUBLIC 实现 S0 拆出的 `backend/src/db/cloud/capture.rs`；修改 `modules/cloud/domain.rs` 的旧黑名单调用入口由 S0 集成。
- PUBLIC 修改 `backend/src/safety/grant/store.rs`；在既有 `backend/src/safety/execution_gateway.rs` 的执行结束边界接入 usage 元数据写入。
- PUBLIC 新增 `backend/src/db/cloud/migrations/2014_usage_projection.sql`、`backend/tests/v11_payload_policy.rs`。
- PRIVATE 新增 `cloud-core/src/payload.rs`、`migrations/0005_usage_projection.sql`、`cloud-core/tests/payload_policy.rs`。
- PUBLIC 新增 `frontend/src/features/cloud/SyncPrivacyPanel.tsx`；更新现有 `docs/privacy/data-classification-v1.1.md`。

## 2. 类型白名单与自由文本边界

不再把“递归删除叫 api_key 的字段”作为主要安全机制。同步序列化必须从业务结构投影为专用 DTO，服务端再按相同 schema 验证；未知字段拒绝，未知类型不能落库。

| 实体 | 允许字段/内容 | 禁止或必须专门处理 |
|---|---|---|
| conversation | schema_version、title、created_at、updated_at | 不同步远端运行中状态；本机恢复为 idle |
| message | schema_version、conversation_id、role=user/assistant、content、created_at | system/developer/tool、工具原始参数、内部提示词不采集 |
| memory | schema_version、content、category、source、source_conversation_id、created_at、updated_at | 任意 metadata Map 必须投影成审核过的结构，不能原样透传 |
| agent_profile | schema_version、name、description、用户明确保存的 instructions、工具/能力 ID 列表、模型 ID、max_iterations、enabled、created_at、updated_at | 提供商 token、env、SecretRef、运行时展开后的系统提示词 |
| workflow_metadata | schema_version、name、description、definition 的类型化节点/边引用、created_at、updated_at | 节点中的任意 JSON 参数、完整执行输出、嵌入凭据 |
| artifact_metadata | schema_version、名称、类型、MIME、大小、用户摘要、实体引用、时间 | 本机绝对路径、文件字节、下载 token |
| task_graph | schema_version、graph_revision、类型化 graph、updated_at | 原始 node_states 执行输出、完整工具结果、未审查 opaque 字段 |
| canvas_view | schema_version、view_revision、graph_revision_seen、viewport、node_layouts、selection、groups、updated_at | 保留现有 stale revision 语义，不改用户视角规则 |
| settings | schema_version、section、该 section 的明确偏好字段 | 模型/语音/搜索密钥、服务认证信息；未知 section 拒绝 |
| capability_usage | schema_version、execution_id、capability_id、plugin_id、version、used_at、execution_status、latency_ms、approval_required、risk_level | 不同步完整输入/输出；result_digest 默认省略，避免低熵正文哈希泄漏 |

工作流/TaskGraph 的嵌套 schema 复用既有类型模型并逐层定义同步投影；无法证明安全可恢复的子字段暂不声明支持，界面显示该类同步不可用。不能清空字段后仍宣称完整恢复原工作流。

以上白名单无法识别用户正文中主动粘贴的所有秘密。产品必须明确哪些用户内容将上传，由用户控制类别；不得把字段校验宣传为“能识别全部敏感信息”。内部 system 消息及凭据配置始终不进入同步。

## 3. 关键 DTO 与验证代码

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMessageRole { User, Assistant }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncMessagePayload {
    pub schema_version: u16,
    pub conversation_id: String,
    pub role: SyncMessageRole,
    pub content: String,
    pub created_at: i64,
}

pub fn validate_message(value: serde_json::Value) -> Result<SyncMessagePayload, &'static str> {
    let message: SyncMessagePayload = serde_json::from_value(value)
        .map_err(|_| "INVALID_SYNC_PAYLOAD")?;
    if message.schema_version != 1 || message.conversation_id.is_empty()
        || message.conversation_id.len() > 160 || message.content.len() > 256 * 1024
        || message.created_at < 0 {
        return Err("INVALID_SYNC_PAYLOAD");
    }
    Ok(message)
}
```

未知 role 和字段由 serde 明确拒绝。总体每项 payload 最大 512 KiB；单字符串/数组长度、引用 ID 格式与嵌套深度分别检查，不能只检查最终 body 大小。delete 必须 payload=null，仍校验 entity_type 在支持列表中。

`validate_sync_payload(entity_type, operation, payload)` 返回 `ValidatedPayload` 枚举，其成员为上表各类型化 payload 或 Tombstone；不能返回原始 Value 假称已校验。wire 输出由该枚举序列化；所有“新增支持”的类型先补 round-trip fixture 再出现在 capabilities。

## 4. 采集和删除不能混淆

当前 capture 通过“以前跟踪过、现在没采集到”推导 delete。改白名单后，如果继续使用这条规则，会把主动排除的 system 消息、关闭的同步类别或尚未支持的新类型错误上传成删除。

本轮必须改为：

- `capture_sync_candidates(ScopedDb, lease, SyncPolicy) -> CaptureReport` 只采集当前 profile、明确允许的实体，返回 captured/skipped_by_policy/rejected 三类计数。
- `record_local_deletion_tx(transaction, EntityKey, local_generation) -> ()` 只在**真实业务删除事务**内登记 tombstone；同步关闭不代表删除。
- 迁移时原跟踪但新策略排除的记录标记 policy_excluded，不生成 delete，不自动删除云副本；清理既有云副本是单独的明确用户操作。
- 删除捕获和业务删除必须同一事务；如果既有业务删除入口暂时无法接入，暂停该实体的自动删除同步并显示限制，不能继续全表缺失推断。

上述接入影响 `db` 中业务删除入口，由 S0 按调用点登记 owner；S6 提交小范围调用补丁，避免在并行阶段自行修改 S2 projection 的相同函数。

## 5. 插件使用历史的真实闭环

```text
既有安全网关实际执行结束
 → record_capability_usage_tx(execution_id, typed metadata)
 → 本地 capability_usage 可查询
 → capture_sync_candidates（类别允许时）
 → 同步 outbox / change_id 稳定
 → 服务端 sync_entities/sync_changes + capability_usage 同一事务
 → pull
 → 本地 usage 去重投影 / 前端只读展示
```

| 函数 | 输入 → 输出 | 约束 |
|---|---|---|
| `record_capability_usage_tx` | tx、UsageRecord → () | execution_id 稳定唯一；只登记实际执行/拒绝/失败结果，不伪造使用成功 |
| `collect_usage_candidates` | tx、SyncPolicy → Vec<SyncSource> | category 未开启就不入队；不读取原始参数/结果 |
| `apply_usage_tx` | tx、owner、ValidatedUsage → () | 服务端在 S1 同一事务中更新 usage 与同步日志 |
| `project_usage_tx` | tx、ValidatedUsage → () | 本地按 execution_id 幂等，不能执行插件或触发工具 |

`UsageRecord` 字段即上表 capability_usage；execution_status 为 succeeded/failed/denied/cancelled，必须来自真实安全网关结果。本阶段测试创建一个无副作用的本地 fixture 工具经过真实网关执行，再检查持久化/采集，不以直接 INSERT 作为唯一证据。

历史工具调用记录若缺少稳定 execution_id、来源或权限结果，不编造回填；明确统计为“历史记录未纳入”。

## 6. 旧授权解析 fail closed

`list_grants` 和 `get_grant` 两条路径都必须取消 effect/permission/resource/source 的默认允许或默认资源。解析任一字段失败时返回带类型的转换错误，安全网关拒绝本次操作并显示 GRANT_RECORD_INVALID。

```rust
fn decode_grant_field<T: serde::de::DeserializeOwned>(
    raw: &str,
    column: usize,
) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
            column, rusqlite::types::Type::Text, Box::new(error)))
}
```

在 effect/permission/source 字段用 `decode_grant_field(...)?`；resource_json 使用 `serde_json::from_str::<GrantResource>` 并传播同类错误。不要简单过滤损坏行：如果损坏行原本是 deny，过滤会撤掉防线。修复前该主体的相关操作应被拒绝，UI 提供可审计的删除/重建授权入口。

继续保留审批、取消、MAX_ITERATIONS、重复工具限制、PermissionManager 和网关审计。不因新账号 profile 缺少授权记录自动赋予高危权限。

## 7. 独立验收

| 用例 | 必须得到的结果 |
|---|---|
| message role=system 或附加 api_key 字段 | 本地不采集/服务端 422，不写同步表 |
| 密钥藏在嵌套未知结构中 | schema 拒绝；不存在黑名单键名才放行的情况 |
| 用户关闭 message 同步 | 不产生 message delete，原本地/云记录不被自动擦除 |
| 本地真实删除 memory | 同一事务保留 tombstone，下一次同步发送 delete |
| 重放 capability execution_id | 本地/云各一条记录，无重复使用次数 |
| corrupt effect、permission、resource、source | 两个 grant 读取路径都报错，真实工具未执行 |
| 合法 allow/deny/到期授权 | 行为保持原有策略，未知数据不扩大权限 |

- [x] S6.1 冻结每类字段 schema 和安全 round-trip fixture。
- [x] S6.2 用严格 DTO 替代主要黑名单路径，修正采集缺失误判删除。
- [x] S6.3 接入网关→usage→outbox→云投影→本地展示闭环。
- [x] S6.4 两条 grant 读取路径 fail closed，补实际网关拒绝测试。
- [x] S6.5 `cargo test -p yilian-backend --test v11_payload_policy`、PRIVATE `cargo test -p yilian-cloud-core --test payload_policy`；双方 fmt/check，前端 test/build。

建议分支 `fix/v1-1-sync-data-policy`；授权修复可拆为 `fix/security-grant-decode`，同属本工作包验收。
