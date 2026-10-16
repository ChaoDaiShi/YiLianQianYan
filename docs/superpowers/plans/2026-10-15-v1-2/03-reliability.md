# 03 R线：可靠性修复与回归

> R线只处理报告已指出的缺陷及其直接回归。当前已有修复先复验，未发现失败时不重写。执行前读取00与02，确认共享文件独占时段。

## 1. R-01：历史成果、来源阈值、完整规划输入

**必读现有文件：** `frontend/src/features/task-world/proofloop/proofloopModel.ts`、`AcceptancePanel.tsx`；`backend/src/modules/task/proofloop/contracts.rs`、`planner.rs`、`drafts.rs`；`backend/tests/proofloop_source_threshold.rs`；`scripts/proofloop/evidence-regression.test.mjs`。

**输入：** 现有历史报告、当前目标契约、实际发送模型的消息。**输出：** 正确展示/错误码/模型调用次数证据；已有实现失败时才改相应逻辑。

- [ ] 先运行现有定向测试，保留每项真实结果，不引用10月13日测试数字。
- [ ] 构造旧报告 `status=pass` 且无 `deliverable_results`；当前目标有交付物；应展示“需重新验收”，数据库原始status不被改写。
- [ ] 用真实缺失文件重新验收，应Fail；恢复真实文件并重新验收才允许Pass。
- [ ] 同时测试编译、验收HTTP和独立汇总器对 `min_sources=0` 的拒绝；不得仅测辅助函数。
- [ ] 构造中文目标与完整schema，分别使最终system/user消息为32000与32001字符；后者返回 `proofloop_planning_input_too_large`，模型服务请求计数为0。
- [ ] 用户显式选择的工具因数量/长度无法完整加入时，返回 `proofloop_capability_selection_incomplete`；不能悄悄删一个工具继续。

最小断言示例（使用现有fixture构造contract，变更字段即可）：

```rust
let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
contract.criteria[0].check = CriterionCheck::SourceTraceability {
    deliverable_id: contract.deliverables[0].id.clone(), min_sources: 0,
};
assert_eq!(contract.validate_source_requirements().unwrap_err().code,
           "proofloop_invalid_source_threshold");
```

此片段必须放在包含现有合法fixture的测试内；不能用空contract绕过实际入口。

## 2. R-02：声明格式与实际字节一致

**拟新增：** `backend/src/modules/task/proofloop/deliverable_format.rs`、`backend/tests/proofloop_format_validation.rs`。**允许修改：** `acceptance.rs`。导出由C接线。

复用已有 `pdf-extract`、`calamine`、`quick-xml`、`zip`、`image` 依赖；`modules/resource/`的预览解析可借鉴或提取共享纯函数，但预览截断不能被当作完整文件验证。不要给此任务增加杀毒平台、通用文件安全框架或后台扫描服务。

**拟新增公共结果：**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatInspection {
    pub mime_type: String,
    pub extracted_text: Option<String>,
    pub text_complete: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub fn inspect_json(bytes: &[u8]) -> Result<FormatInspection, super::contracts::ProofError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e|
        super::contracts::ProofError::new("proofloop_deliverable_format_mismatch", e.to_string()))?;
    Ok(FormatInspection {
        mime_type: "application/json".into(),
        extracted_text: Some(value.to_string()), text_complete: true,
        width: None, height: None,
    })
}
```

主签名：`inspect_deliverable_format(mime: &str, name: &str, bytes: &[u8]) -> Result<FormatInspection, ProofError>`。由R实现以下确定分派表：

| 声明类型 | 必须做的检查 | 不能替代的检查 |
|---|---|---|
| `text/plain`、`text/markdown` | `std::str::from_utf8`成功且正文非空；章节沿用原检查 | 扩展名、file exists |
| `application/json` | 完整JSON解析成功；内容要求按目标条件 | 包含花括号/只截前缀 |
| DOCX MIME | ZIP可读；`[Content_Types].xml`、`_rels/.rels`、`word/document.xml`存在；XML可读、提取正文 | 仅有PK魔数/一个XML条目 |
| `application/pdf` | PDF完整解析成功；需要文字的目标必须有可提取文字 | `%PDF`头、零字节尾巴 |
| XLSX MIME | `calamine`读取工作簿与工作表成功；要求的数据单元格真实存在 | 把CSV改名xlsx |
| `image/png` | `image`完整解码、尺寸大于0；期望尺寸按目标约束核对 | 只读PNG头部尺寸 |
| 未支持MIME | 返回 `proofloop_deliverable_format_unsupported`，该交付物NotChecked，目标Incomplete | 自动降为text并Pass |

DOCX/XLSX MIME使用标准完整值，不新增缩写协议。格式错误返回 `proofloop_deliverable_format_mismatch` → DeliverableResult Fail。解析能力不支持是NotChecked；二者不可混淆。

- [ ] 先为“真实txt字节但声明PDF”“截断PNG”“普通ZIP但声明DOCX”写失败测试。
- [ ] 逐格式实现，使用真实合法小文件正例；每个正例检查内容，不能只检查函数没报错。
- [ ] 在 `evaluate_deliverables` 完成原来源/摘要检查后调用格式检查，保持原evidence_ids与artifact_id关联。
- [ ] `RequiredSections`读取格式解析出的正文；如果正文不完整，缺章节不能推断为确实缺失，应NotChecked并说明解析范围；不将二进制按UTF-8解释。
- [ ] 所有路径继续验证当前goal_version、graph_revision、execution_id；不得用另一次执行的合法PDF替代本次错误文件。
- [ ] 增加HTTP验收测试：假PDF + 人工pass仍不得把目标变Pass。

单测示例：

```rust
assert!(inspect_deliverable_format("application/pdf", "report.pdf", b"ordinary text").is_err());
assert!(inspect_deliverable_format("application/json", "report.json", b"{broken").is_err());
assert!(inspect_deliverable_format("application/json", "report.json", br#"{"ok":true}"#).is_ok());
```

## 3. R-03：云同步开关竞态

独占现有 `backend/src/api/cloud.rs`、`backend/tests/v11_conflict_http.rs`。这是公共客户端代码，不能据此修改私有服务。

- [ ] 复用现有测试建真实AppServer和独立profile数据库。
- [ ] 持有 `cloud_operation_lock`，分别调用sync choice与sync now HTTP入口，断言409且旧policy不变。
- [ ] 释放锁后重试一次，断言policy真实落库；并发两个请求只按现有锁语义成功/冲突，不返回两个虚假成功。
- [ ] 锁测试不能代替两设备同步；两设备仍在发布清单单列。

## 4. R-04：真实SQLite写失败、恢复和预算

拟新增 `backend/tests/proofloop_db_faults.rs`；现有预算测试可在 `budget/tests.rs`补充。仅在测试专用数据库注入，证据目录为 `target/v12-evidence/<run-id>/r04/`。不改生产数据库，不删除用户文件，不为测试增加生产故障开关。

已核实真实表名为 `proofloop_time_usage`。先创建活动lease，再安装以下测试数据库触发器，使结束写入真实失败：

```sql
CREATE TRIGGER v12_fail_time_update
BEFORE UPDATE ON proofloop_time_usage
BEGIN
  SELECT RAISE(ABORT, 'v12_injected_write_failure');
END;
```

触发器必须在专用库初始化和现有 `begin_goal_execution(server, graph_id, execution_id)` 返回真实BudgetLease后安装；结束活动不得先把内存count清零。模块内测试可使用已有 `GoalBudgetMeter::begin`。测试结束只在同一专用库执行 `DROP TRIGGER v12_fail_time_update`。

- [ ] 执行真实 `BudgetLease::finish`，捕获失败后 `GoalBudgetMeter::check` 的 `proofloop_usage_unavailable`。
- [ ] 比较数据库与内存可观察结果：未持久化结束不能被视为有完整剩余预算。
- [ ] 移除触发器，验证已有可恢复结算路径；无法证明完整计量时保持未知，不凭空清零。
- [ ] 用两个真实SQLite连接，第一连接 `BEGIN IMMEDIATE` 持写锁，第二连接走真实HTTP保存入口；验证busy失败、事务回滚、重试状态。
- [ ] 重建AppServer打开相同测试库，验证 `interrupted_unknown` 与原attempt记录；不自动重跑外部结果未知节点。
- [ ] 若计划使用磁盘满/只读文件系统，必须建立专用测试卷/目录；未具备环境就标NOT TESTED，不能用字符串异常声称完成该故障。
- [ ] 保存退出码、SQL前后状态、HTTP响应和测试库副本；副本不可含真实凭据。

## 5. R线验收编号

| 编号 | 输入/动作 | 必须观测 |
|---|---|---|
| R01–R03 | 旧报告、缺结果、重新验收 | 原历史保留；UI不绿灯；fresh report才恢复 |
| R04–R06 | 假PDF、缺OOXML结构、截断PNG | 格式Fail，不能被人工Pass覆盖 |
| R07–R10 | 合法JSON/DOCX/XLSX/PDF、未知MIME | 合法文件可解析；未知NotChecked；正文与摘要正确 |
| R11–R12 | 云锁占用、释放后重试 | 409且无写入；随后实际成功 |
| R13–R14 | 零来源阈值进编译/验收/统计 | 同一错误语义，模型成功不覆盖 |
| R15–R18 | 32000/32001字符、中文、显式能力超预算 | 按最终Unicode消息；无静默遗漏 |
| R19–R23 | UPDATE失败、busy、重启、并行活动、审批等待 | 保留未知；并行按区间并集；等待不算active_work |
| R24–R25 | 结果未知节点恢复、旧草稿并发确认 | 不重放未知副作用；CAS冲突409 |
| R26–R27 | 安装升级、双设备同步 | 专用真实环境证据；没有就NOT TESTED |

## 6. 验证命令

在仓库根执行；新增测试文件完成后才运行对应命令。

```powershell
cargo test -p yilian-backend --test proofloop_source_threshold -j 1
cargo test -p yilian-backend --test proofloop_delivery_integrity -j 1
cargo test -p yilian-backend --test v11_conflict_http -j 1
cargo test -p yilian-backend --test proofloop_format_validation -j 1
cargo test -p yilian-backend --test proofloop_db_faults -j 1
node --test scripts/proofloop/evidence-regression.test.mjs
npm.cmd --prefix frontend test -- src/features/task-world/proofloop/proofloopModel.test.ts src/features/task-world/proofloop/AcceptancePanel.test.tsx
```

Rust修改还要按13运行fmt/check和相关测试，前端修改要production build。所有命令记录非零测试数量；“0 tests”不是通过。
