# 11 外部智能体与MCP扩展

> M线是独立扩展，不阻塞Core-v1.2。先完成一个真实外部智能体的标准会话与成果回传，不实施多桌面嵌入、任意软件挂载或自动安装编码工具。

## 1. M-01：一个ACP v1提供方

拟新增 `backend/src/integrations/acp/mod.rs`、`model.rs`、`client.rs`、`session.rs`、`tests.rs`，以及 `backend/tests/acp_adapter.rs`。注册、配置和Task Harness连接归C。复用既有进程启动、日志、凭据引用及取消方式；外部进程只在需要时启动。

执行前固定一个用户已经授权、机器实际可运行、声明支持ACP v1的提供方，记录可执行路径、版本、参数和协议初始化回包。不能仅因软件名包含“Agent”或能输出JSON就宣称支持ACP。没有合格提供方时协议单测可交付，GM真实验收标 `BLOCKED_ENV`。

官方v1初始化使用整数 `protocolVersion: 1` 并协商能力；基础会话包括new、prompt、update和cancel。客户端只声明实际实现的文件/终端能力。[ACP v1初始化规范](https://agentclientprotocol.com/protocol/v1/initialization)

最小初始化报文：

```json
{
  "jsonrpc":"2.0",
  "id":1,
  "method":"initialize",
  "params":{
    "protocolVersion":1,
    "clientCapabilities":{},
    "clientInfo":{"name":"YiLianQianYan","version":"v12-adapter-pilot"}
  }
}
```

首版默认不声明client terminal/fs能力；如果选定提供方必须调用这些能力，C将相应请求映射至已有Gateway和审批，并通过真实测试后才能声明。不能在ACP client里另写不经审批的shell执行函数。

拟新增内部状态：

```rust
pub enum ExternalSessionState {
    Starting,
    Ready,
    Running,
    WaitingApproval,
    Cancelling,
    Completed,
    Failed,
    ResultUnknown,
}
pub struct ExternalTaskResult {
    pub session_id: String,
    pub state: ExternalSessionState,
    pub artifact_paths: Vec<String>,
    pub model_calls: Option<u32>,
    pub total_tokens: Option<u64>,
    pub error_code: Option<String>,
}
```

这些是内部状态，wire字段按官方schema，不自创协议。`session/prompt`返回结束原因只表示该轮停止，成果仍需读取真实文件并走证行验收。[ACP v1 Prompt Turn](https://agentclientprotocol.com/protocol/v1/prompt-turn)

- [ ] M-01a：完成stdio JSON-RPC请求ID关联、initialize版本协商、session/new与session/update解析；stderr仅作日志，不能混入协议stdout。
- [ ] M-01b：在任务专用隔离目录提交一个生成小文件的任务，记录进度、工具审批、结束原因；读取真实文件，计算摘要并导入原ArtifactService。
- [ ] M-01c：父取消映射session/cancel并等待终态；进程异常退出且外部结果不明时ResultUnknown，不自动新开会话重复任务。
- [ ] M-01d：接D资源槽，首版并行外部会话最多1；无usage时保留unknown，受目标截止时间和已知调用预算控制，不能填0消耗。
- [ ] 仅收集任务隔离目录内预期成果。第三方agent的自主文件/网络能力必须由其真实隔离配置约束；若无法证明隔离，首版只允许只读/生成任务，不宣称忆涟Gateway能拦截其内部所有动作。
- [ ] 会话完成后回收本任务启动的进程；不能按进程名全局结束其他用户实例。

## 2. GM验收

| 编号 | 场景 | 通过证据 |
|---|---|---|
| M01 | v1初始化成功/不支持版本 | 正确会话或明确不支持；不能假降级为自定义协议 |
| M02 | 真实外部agent生成指定文件 | 进度、文件实际字节、摘要、证行报告 |
| M03 | 外部请求未声明能力 | 不执行，明确错误 |
| M04 | 父取消和进程退出 | 会话终态、无后续写入、资源槽释放 |
| M05 | 断线后结果未知 | 不自动重发有副作用任务 |
| M06 | 无usage提供方 | unknown字段保留，成本报告不虚报低成本 |

```powershell
cargo test -p yilian-backend --test acp_adapter -j 1
```

模拟ACP服务只证明协议适配，不通过M02。ACP v2继续作为独立研究范围，不把草案支持加入本轮稳定兼容声明。[ACP v2迁移范围](https://agentclientprotocol.com/protocol/v2/migration)

## 3. M-02：复查已有MCP版本分流，选择一个任务扩展

必读现有 `backend/src/integrations/mcp/protocol/version.rs`、`model.rs`、`transport/http.rs`、`transport/stdio.rs`及测试。已存在2026-07-28和2025-11-25标识，不能再创建第二套版本协商模块。

本工作包第一步是运行现有现代/legacy传输测试并记录实际支持矩阵。只补本次长任务需要的Tasks扩展；MCP Apps交互嵌入无首版用户任务，不在本轮新增WebView/应用市场，只在支持矩阵注明未实现。

Tasks扩展有独立规范版本和能力标识；实现时以锁定版本的schema为准，不混用旧实验核心任务API。完成结果、待输入和任务句柄必须按resultType分别处理。[MCP Tasks 2026-07-28规范](https://modelcontextprotocol.github.io/ext-tasks/specification/2026-07-28/tasks.html)

拟新增 `backend/src/integrations/mcp/protocol/task_extension.rs`、`backend/tests/mcp_task_extension.rs`；旧transport/registry接线交C。

- [ ] 锁定官方schema版本并在测试fixture保存版本来源与哈希，不把浮动draft当稳定依赖。
- [ ] 对支持该扩展的提供方才声明能力；普通工具返回complete继续走原路径。
- [ ] task句柄关联当前目标/节点/execution；用tasks/get轮询、需要输入时按该版本tasks/update提交、取消时调用tasks/cancel。
- [ ] 任务仍在运行时不能把句柄当最终artifact；完成后取真实结果进入原证据链。
- [ ] 轮询等待时间与实际执行活动按现有时间基准记录；不得仅为等待而重置10分钟预算。
- [ ] 中断后可查任务状态则查询；无法确认外部结果时保持未知，不自动重复tools/call。
- [ ] legacy服务器不支持扩展时明确普通调用支持范围；本轮不删除旧传输。

MT01普通工具兼容、MT02长期任务完成、MT03输入请求、MT04取消、MT05断连/未知状态、MT06legacy回归都要有真实请求轨迹。Tasks扩展不是ACP前置，两项可以并行开发并分别验收。
