# E 云同步冲突三态接线

> 执行方式：使用 `executing-plans`。状态：CODE_IMPLEMENTED。优先级：P1。对应 R06。与 ProofLoop A—D 无代码依赖，可独立开发、验收和集成。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 账号中心真实支持保留本地、采用云端、另存本地副本；使用实际冲突版本和本地代次，不再把副本选择降为旧二态接口。

**架构：** 消费本地后端已经返回的 `records`，调用现有 `choice` 分支，复用 `ConflictService` 和 SQLite 冲突解决事务。

**技术栈与约束：** React/TypeScript、Rust/Axum、客户端 SQLite。默认只修改公共客户端，不要求私有服务器增加接口、改管理后台或重新部署。

## 1. 已有能力与真正缺口

`GET /api/cloud/sync/conflicts` 当前已返回 `{ conflicts, records }`。其中 conflicts 是旧列表；records 来自 `ConflictRecord`，包含版本、代次、状态、request ID、副本 ID 和 queued change ID。

`POST /api/cloud/sync/conflicts/:id/resolve` 已接受 `choice`、`expected_conflict_version`、`expected_remote_revision`、`request_id`，并调用 `resolve_conflict`。旧客户端只提交 resolution；账号中心也将 `keep_local_as_copy` 转成 local，并把版本和代次填成 1。

本地事务已验证当前实体的 local_generation 与冲突记录是否一致。**不要为了去掉前端固定值，另造一个客户端可自报的 expected_local_generation 字段**；显示使用 records 中的真实值，实际裁决继续由数据库完成。

## 2. 文件与函数契约

| 改动 | 文件 |
|---|---|
| 强类型 records、三态请求与响应 | `frontend/src/api/cloud.ts` |
| 列表状态和真实三态调用 | `frontend/src/pages/AccountCenterPage.tsx` |
| DTO 适配、显示与选项校验 | `frontend/src/features/cloud/conflictModel.ts`、`ConflictResolutionPanel.tsx` |
| 请求构造与同一决策 ID 复用（新增） | `frontend/src/features/cloud/conflictRequest.ts`、`conflictRequest.test.ts` |
| 三态分支必需字段校验 | `backend/src/api/cloud.rs` |
| 接口与事务回归（新增） | `backend/tests/v11_conflict_http.rs` |
| 真实点击场景 | V 统一修改 `frontend/e2e/cloud-reliability.mjs` |

现有 `backend/src/modules/cloud/contracts.rs::ConflictRecord` 是类型真值。前端增加完整对应的 `CloudConflictRecord`；状态联合为 open/resolving/resolved，nullable 字段用 `string | null`，不靠 `any` 或默认值填补。

```ts
export type ConflictChoice =
  | "keep_local"
  | "use_remote"
  | "keep_local_as_copy";

export interface ResolveConflictChoicePayload {
  choice: ConflictChoice;
  expected_conflict_version: number;
  expected_remote_revision: number;
  request_id: string;
}

export interface ConflictResolutionResult {
  conflict_id: string;
  state: "open" | "resolving" | "resolved";
  queued_change_id: string | null;
  copied_entity_id: string | null;
}

export interface ConflictResolutionResponse {
  result: ConflictResolutionResult;
  conflicts: SyncConflict[];
  records: CloudConflictRecord[];
}

export function resolveConflictChoice(
  id: string,
  payload: ResolveConflictChoicePayload,
): Promise<ApiResult<ConflictResolutionResponse>> {
  return requestResult("POST", `/api/cloud/sync/conflicts/${id}/resolve`, payload);
}
```

`SyncConflict`、`ApiResult`、`requestResult` 已存在于该 API 文件及其导入中；`CloudConflictRecord` 按前述 Rust DTO 定义，不自行删掉版本与状态字段。`listSyncConflicts` 返回类型补上 `records: CloudConflictRecord[]`。旧 resolveSyncConflict 保留给兼容调用，本页正式三态操作全部改用新函数。

示例请求与当前后端 DTO 对应：

```json
{
  "choice": "keep_local_as_copy",
  "expected_conflict_version": 4,
  "expected_remote_revision": 12,
  "request_id": "b72db216-59b7-4edb-992c-bf43c28772b0"
}
```

## 3. 前端接线与幂等

账号中心的状态存储 records，直接投影为 ConflictItem；统一类型定义或使用明确适配，不保留 `conflict_version: 1`、`local_generation: 1`、`status: "open"` 等伪造字段。

`conflictRequest.ts` 的纯构造函数：

```ts
export function buildConflictChoice(
  conflict: Pick<CloudConflictRecord, "conflict_version" | "remote_revision">,
  choice: ConflictChoice,
  requestId: string,
): ResolveConflictChoicePayload {
  return {
    choice,
    expected_conflict_version: conflict.conflict_version,
    expected_remote_revision: conflict.remote_revision,
    request_id: requestId,
  };
}
```

三个类型从 `frontend/src/api/cloud.ts` 以 type import 引入。choice 原样传递；requestId 在用户选择开始时生成一次，而不是在 API 函数每次发送时生成。

同一未完成决策以 `local_identity_id + account_id + conflict.id + conflict_version + choice` 标识，保存 request_id 与请求字段。网络失败重试复用该请求；不把重试变成新决策。页面重开先加载 records，已有 resolution_request_id 时恢复实际状态。若需要跨页面保留尚未得到响应的 request_id，仅在 sessionStorage 保存这些标识与请求字段，不保存本地/云端正文；退出或身份变化后清理当前请求状态。

提交期间禁用重复点击；失败后保留冲突内容和原选择。发生 CONFLICT_CHANGED 时刷新真实记录，让用户重新比较并重新选择，此时才产生新的 request_id。刷新失败显示错误并保留原列表，不能清空为“无冲突”。

## 4. 后端最小改动与 UI 状态

新 choice 分支要求三个参数都存在：expected_conflict_version、expected_remote_revision、非空 request_id。取消该分支的 `unwrap_or(1)` 和自动生成 UUID；缺字段返回 400 `invalid_conflict_request`。旧 resolution 分支继续兼容现有明确的 local/remote 调用；请求不能同时携带 choice 与 resolution 让语义产生歧义。

继续调用当前 ScopedDb/lease 和 `resolve_conflict_tx`，后者已经校验版本、代次、操作与副本引用。不要在前端发送第三种 choice 后仍调用旧 DB 二态包装方法。

| 返回/实际状态 | 产品文案与行为 |
|---|---|
| open | 展示两侧内容，允许合法选择 |
| resolving | “选择已提交，正在等待同步确认”，保留记录，不再次创建选择 |
| resolved 且有 copied_entity_id | “本地副本已创建”，提供新副本 ID/打开入口；上传进度仍以同步中心实际状态为准 |
| resolved 且无副本 | “本地冲突已处理”，不据此宣称两台设备全部一致 |
| 版本/本地代次变化 | “内容已变化，请刷新后重新选择” |
| records 为空 | “当前没有待处理的同步冲突”，不能显示“所有云端与本地完全一致” |

当前 KeepLocalAsCopy 事务会创建新 ID/base_revision=0 的 upsert、保留原 ID 的云端版本，并将本地冲突标记 resolved。**这不是云端已经 ACK 的证明**。本轮不为统一文案擅改后端状态机；UI 根据副本、队列与实际同步结果分别展示。

## 5. 用例与代码负例

```ts
import { describe, expect, it } from "vitest";
import { buildConflictChoice } from "./conflictRequest";

describe("conflict choice request", () => {
  it("keeps copy choice and actual versions", () => {
    expect(buildConflictChoice(
      { conflict_version: 4, remote_revision: 12 },
      "keep_local_as_copy", "decision-1",
    )).toEqual({
      choice: "keep_local_as_copy",
      expected_conflict_version: 4,
      expected_remote_revision: 12,
      request_id: "decision-1",
    });
  });
});
```

该测试必须配合账号中心真实点击和后端事务测试：

| 编号 | 场景 | 必须看到的结果 |
|---|---|---|
| E01 | 冲突 version=4、local_generation=7，点击另存副本 | 实际 HTTP choice 为 keep_local_as_copy；页面显示真实 4/7 |
| E02 | 副本选择成功 | 新实体 ID 不同；原 ID 应用云端版本；新 ID 的 outbox base=0 |
| E03 | 同一 request_id 重发、双击、响应丢失后重试 | 同一个 copied_entity_id、同一个 queued_change_id，不新增第二份副本 |
| E04 | 选择后本地被再次编辑或远端版本变化 | 旧决策拒绝，保留新内容并刷新 |
| E05 | 云端 delete、本地 upsert | keep_local 被拒绝；副本用新 ID，原 tombstone 不复活 |
| E06 | keep_local / use_remote | 使用各自正确语义；不因三态接线破坏已有行为 |
| E07 | 网络失败、records 请求失败 | 不显示成功，不清空冲突列表，不改变已知版本 |
| E08 | 三态请求缺版本/request_id，或混合新旧字段 | 返回 400，不修改任何实体/队列 |
| E09 | resolved 副本仍待上传 | 显示副本已创建，不能写“云端同步完成” |
| E10 | 两设备同账号操作真实测试数据 | 第二台设备收到新 ID 的副本；原云端实体/删除语义正确 |

E10 进入 V 的真实两设备验收，不能用一个浏览器的两个标签页替代。私有服务若出现真实协议错误，保留请求和响应证据后单独建服务端修复包；不提前扩大到管理员端。

- [ ] 先增加 E01、E02 负例，确认不是仅按钮标签错误。
- [ ] 接 records 和强类型三态请求，再接版本错误与幂等请求 ID。
- [ ] 完成状态文案和副本打开，保留列表失败状态。
- [ ] 执行 `cargo test -p yilian-backend --test v11_conflict_http --test v11_conflict_recovery -- --test-threads=1`、fmt/check。
- [ ] 在 frontend 执行 `npm.cmd test -- src/features/cloud/conflictRequest.test.ts src/features/cloud/conflictModel.test.ts src/pages/AccountCenterPage.test.tsx` 及 `npm.cmd run build`。
- [ ] 真实浏览器验证 E01—E09，交接 E10 的数据与设备前置条件。
- [ ] 提交 `fix(sync): wire conflict records and three-way resolution`。

通过条件：E01—E09 可独立验收；E10 未完成时明确标为 NOT_TESTED，不能把客户端修复通过等同于云端两设备验收完成。
