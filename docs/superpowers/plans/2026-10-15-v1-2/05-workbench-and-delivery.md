# 05 目标工作台与成果交付

> U负责界面，O负责格式生成。先用C提供的固定夹具独立开发组件，随后接真实API。不得修改已有画布几何、镜头、节点拖动或选择状态来实现目标视图。

## 1. U-01：同一任务的目标/专家视图

**必读现有：** `frontend/src/features/task-world/TaskWorldPage.tsx`、`proofloop/GoalComposer.tsx`、`PlanReviewPanel.tsx`、`AcceptancePanel.tsx`、`proofloopModel.ts`、`frontend/src/api/proofloop.ts`。

**拟新增独占：** `frontend/src/features/task-world/goal-view/GoalOverview.tsx`、`GoalProgress.tsx`、`GoalBlockers.tsx`、`DeliverableList.tsx`、`goalViewModel.ts`及对应测试；`frontend/src/styles/proofloopGoalView.css`。TaskWorldPage中的开关与接线交C。

固定布局：顶部目标/当前方案；中部按执行阶段列进度与需要操作事项；下部成果列表。专家视图沿用现有画布。按钮文案为“目标视图”“专家视图”“查看方案”“查看成果”“查看执行记录”，不把内部ID作为首要标签。

拟新增局部投影类型：

```ts
export type GoalViewState = {
  graphId: string;
  title: string;
  phase: "planning" | "needs_input" | "ready" | "running" | "waiting_approval" |
    "verifying" | "completed" | "failed" | "cancelled" | "needs_revalidation";
  completedNodes: number;
  totalNodes: number;
  blockers: { code: string; message: string; nodeId: string | null }[];
  deliverables: {
    id: string; title: string; artifactId: string | null;
    state: "pass" | "fail" | "not_checked" | "human_pending" | "stale";
    reason: string;
  }[];
};
```

`buildGoalViewState`输入：现有ProofDraft、当前TaskGraph、最新NodeExecution列表、当前AcceptanceReport或null；输出上述纯投影。类型导入沿用现有源文件，不复制TaskGraph定义。阶段优先级冻结：取消→需重新验收/过期→待审批/待输入→运行中→验证中→已验收完成/失败。没有报告不能仅因节点全绿显示completed。

- [ ] 先为“节点全部成功但成果缺失”“历史Pass但需重验”“待审批且预算未计等待时间”编写模型测试。
- [ ] 组件只接props，不在每个卡片内单独fetch同一图；由页面现有加载/更新流供数。
- [ ] 切换视图只改变显示模式，graph_id、选中节点、任务结果不变；同一图不要另建Zustand事实存储。
- [ ] 刷新后默认视图可用现有偏好机制保存；不新增云设置系统。
- [ ] 任何HTTP失败显示真实错误并保留已有输入；不生成默认“成功”卡片。

## 2. U-03：候选方案比较与确认

拟新增 `frontend/src/features/task-world/proofloop/PlanComparisonPanel.tsx`及测试，消费01中的PlanningReview。

每张卡显示：方案标题、交付物覆盖、步骤数、风险、预计成本是否已知、缺失输入、排除原因。用户能展开每一步的工具与参数。推荐项只是建议，不能自动跳过计划确认或工具审批。

选中调用 `select-candidate`；API返回新revision后再更新卡片。409保留当前用户选择意图、重新读取最新草稿并提示版本已变化，不能用旧revision反复写。confirm仍调用原端点。

关键交互验收：方案B原本非推荐但合法，选择后保存重开仍为B；方案A无效，即使分数高也不可确认；没有Ready方案时显示具体缺输入/诊断而不是无限loading。

## 3. U-02：成果列表和实际打开

`DeliverableList`消费后端验收结果及现有artifact记录；只在artifact存在时显示“打开”。打开动作复用已有Tauri/资源授权接口，不把任意模型文本当本地路径交给shell。

每项显示：用户标题、格式、大小、生成时间、验收状态、失败原因、来源入口。专家展开可以显示execution_id和摘要；普通目标视图不强迫用户理解ID。

- [ ] 用户要求PDF，列表必须对应真实PDF字节；Markdown不能改后缀交付。
- [ ] 无文件、文件被删除、摘要改变、格式不符都显示相应真实状态。
- [ ] 浏览器验收可测下载与解析；“桌面打开成功”另由Tauri实际窗口/文件关联证据证明，不能用Web下载替代。
- [ ] 用户取消执行后保留已有成果和历史，不自动删除工作区文件。

## 4. O-01：最小真实格式生成

**必读现有：** `backend/src/modules/task/artifact.rs`、`backend/src/db/artifact_provenance.rs`、现有 `deliverable_nodes` 与物化路径。**拟新增：** `backend/src/modules/task/deliverable_renderers/mod.rs`、`text.rs`、`json.rs`、`backend/tests/deliverable_renderers.rs`；模块导出和ArtifactService接线归C。

生成器只处理明确结构化内容→真实字节；调用原 `ArtifactService::materialize`写文件、关联本次执行和来源。不要另建成果存储，不让生成器自报目标Pass。

```rust
pub struct RenderedDeliverable {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub fn render_json(file_name: &str, value: &serde_json::Value)
    -> Result<RenderedDeliverable, serde_json::Error>
{
    Ok(RenderedDeliverable {
        file_name: file_name.to_owned(),
        mime_type: "application/json".into(),
        bytes: serde_json::to_vec_pretty(value)?,
    })
}
```

`render_markdown(file_name: &str, text: &str) -> RenderedDeliverable`直接UTF-8编码正文；内容由实际工具/任务输出提供，不内置假报告。生成后R-02执行格式和内容验证，再由原验收产生结果。

## 5. O-02：DOCX、PDF、XLSX、PNG逐格式交付

固定顺序是格式间的实现建议，不阻塞U和O-01：DOCX→XLSX→PNG→PDF。各自独占新增 `docx.rs`、`xlsx.rs`、`png.rs`、`pdf.rs`，并各有真实文件fixture。

| 格式 | 首版生成范围 | 复用/实现方法 | 必须独立核验 |
|---|---|---|---|
| DOCX | 标题、段落、简单表格、来源列表 | 已有zip和XML库写合法OOXML包；明确关系与内容类型 | R解析正文 + LibreOffice或Word实际打开/转换 |
| XLSX | 一个命名工作表、文本和数值、表头 | 已有zip/XML写最小合法OOXML；首版不生成公式 | calamine读回坐标、值、行列数；Excel/LibreOffice打开 |
| PNG | 由结构化数值生成固定尺寸柱状图或既有截图 | 已有image编码；图表布局可用已有前端导出后物化 | 完整解码、尺寸、与原数据核对 |
| PDF | 由同一DOCX/HTML内容生成含中文文本的文档 | 使用用户已配置且实际可用的本地渲染器；运行经原工具执行机制 | 完整解析、中文正文、页数、实际打开 |

没有可用PDF渲染器时，O-02-PDF标 `BLOCKED_ENV` 并在产品中说明“当前环境未配置PDF生成”；不自动安装全局软件、不静默交付另一个格式。新增专用依赖必须由C说明必要性和锁定版本，不能为小表格引入整套新后端。

结构化输入冻结为 `DocumentContent { title: String, sections: Vec<DocumentSection>, sources: Vec<String> }`、`DocumentSection { heading: String, paragraphs: Vec<String>, table: Option<Vec<Vec<String>>> }`；XLSX输入 `SheetContent { name: String, headers: Vec<String>, rows: Vec<Vec<serde_json::Value>> }`。不得把带任意脚本的模型HTML直接当可信执行代码。

## 6. 验收和命令

| 编号 | 场景 | 结果 |
|---|---|---|
| U01 | 同一图切换双视图、刷新 | graph_id和状态一致，画布镜头不被重置 |
| U02 | 缺成果/旧报告 | 目标视图不能显示已完成 |
| U03 | 候选选择、CAS冲突、待输入 | 真实API状态与卡片一致 |
| U04 | 1280×800与1920×1080窗口 | 关键按钮可见，无主操作被横向裁切 |
| O01 | Markdown/JSON链路 | 真字节、真摘要、真执行来源、可打开 |
| O02–O05 | DOCX/PDF/XLSX/PNG分别生成 | 每格式均有生成、解析、内容、打开证据 |

```powershell
npm.cmd --prefix frontend test -- src/features/task-world/goal-view src/features/task-world/proofloop/PlanComparisonPanel.test.tsx
npm.cmd --prefix frontend run build
cargo test -p yilian-backend --test deliverable_renderers -j 1
```

UI自动化由E接入已有core-paths运行器，不安排真人用户测试；真实视觉效果以截图/DOM/交互记录验收，不宣称证明主观易用性。
