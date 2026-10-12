import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';

// Controlled fixtures exercise the actual product LLM transport, MCP runtime,
// typed binding resolver, persistent approvals, repair dispatch and artifacts.
// They are explicitly not external-provider or business-benchmark evidence.
export async function proofloop({ page, frontendPort, backendPort, controlToken, evidenceDir }) {
  const calls = [], requests = [], steps = [], modelCalls = [];
  let available = true;
  const uri = 'https://example.com/proofloop-controlled-fixture';
  const schema = { type: 'object', properties: { sources: { type: 'array', items: { type: 'object', properties: { uri: { type: 'string' }, retrieved_at_ms: { type: 'integer' } }, required: ['uri', 'retrieved_at_ms'] } } }, required: ['sources'] };
  const tools = [
    { name: 'collect_sources', description: 'CONTROLLED FIXTURE collect', inputSchema: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] }, outputSchema: schema },
    { name: 'build_report', description: 'CONTROLLED FIXTURE report', inputSchema: { ...schema, properties: { ...schema.properties, title: { type: 'string' } }, required: ['sources', 'title'] }, outputSchema: schema },
  ];
  const mcp = createServer(async (req, res) => {
    let body = ''; for await (const chunk of req) body += chunk;
    const rpc = JSON.parse(body || '{}'); let result = {};
    if (rpc.method === 'server/discover') result = { supportedVersions: ['2026-07-28', '2025-11-25'], serverInfo: { name: 'PROOFLOOP CONTROLLED MCP', version: '1' }, capabilities: available ? { tools: true } : {} };
    if (rpc.method === 'tools/list') result = { tools };
    if (rpc.method === 'tools/call') {
      calls.push({ tool: rpc.params.name, arguments: rpc.params.arguments });
      const sources = rpc.params.name === 'collect_sources' ? [{ uri, retrieved_at_ms: Date.now() }] : rpc.params.arguments.sources;
      result = { content: [{ type: 'text', text: 'CONTROLLED FIXTURE actual output' }], structuredContent: { sources, report: `# Controlled report\n\n## Sources\n${uri}\n` }, isError: false, resultType: 'complete' };
    }
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ jsonrpc: '2.0', id: rpc.id, result }));
  });
  await new Promise((resolve) => mcp.listen(0, '127.0.0.1', resolve));
  const fixture = { caps: [] };
  const model = createServer(async (req, res) => {
    let body = ''; for await (const chunk of req) body += chunk;
    const request = JSON.parse(body || '{}'); modelCalls.push({ at: Date.now(), messages: request.messages?.length });
    const capability = (name) => fixture.caps.find((item) => item.name === name)?.id;
    const response = {
      deliverables: [{ id: 'report-file', title: 'proofloop-report.md', mime_type: 'text/markdown' }],
      criteria: [
        { id: 'exists', description: '真实文件存在', required: true, check: { kind: 'artifact_exists', deliverable_id: 'report-file' } },
        { id: 'digest', description: '文件摘要一致', required: true, check: { kind: 'artifact_digest_matches', deliverable_id: 'report-file' } },
        { id: 'sources', description: '至少一个实际引用', required: true, check: { kind: 'source_traceability', deliverable_id: 'report-file', min_sources: 1 } },
        { id: 'human', description: '人工阅读确认', required: true, check: { kind: 'human_review', deliverable_id: 'report-file', rubric: '阅读真实受控成果并确认引用' } },
      ],
      candidate: { // Intentionally follows the original prompt without schema_version.
        nodes: [
          { id: 'collect', title: 'Collect sources', instruction: 'Collect controlled references', capability_id: capability('collect_sources'), literal_input: { query: 'controlled proofloop review' }, bindings: [] },
          { id: 'report', title: 'Build report', instruction: 'Build a report with sources', capability_id: capability('build_report'), literal_input: { title: 'Controlled report' }, bindings: [{ target_path: ['sources'], source: { kind: 'dependency_output', node_id: 'collect', path: ['structured', 'sources'] } }] },
        ], edges: [{ from: 'collect', to: 'report' }], deliverable_nodes: [{ deliverable_id: 'report-file', node_id: 'report' }],
      },
    };
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ id: 'controlled-model', choices: [{ index: 0, finish_reason: 'stop', message: { role: 'assistant', content: JSON.stringify(response) } }], usage: { prompt_tokens: 20, completion_tokens: 12, total_tokens: 32 } }));
  });
  await new Promise((resolve) => model.listen(0, '127.0.0.1', resolve));
  const api = async (method, path, body, expected = 200) => {
    const response = await fetch(`http://127.0.0.1:${backendPort}${path}`, { method, headers: { 'Content-Type': 'application/json', 'X-Yilian-Control-Session': controlToken }, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await response.text(); requests.push({ method, path, status: response.status });
    assert.equal(response.status, expected, `${method} ${path}: ${text}`); return text ? JSON.parse(text) : null;
  };
  const until = async (read, accept, label) => {
    const deadline = Date.now() + 20000; let latest;
    while (Date.now() < deadline) { latest = await read(); if (accept(latest)) return latest; await new Promise((resolve) => setTimeout(resolve, 100)); }
    throw new Error(`${label}: ${JSON.stringify(latest)}`);
  };
  const capture = (name) => page.screenshot({ path: join(evidenceDir, `proofloop-${name}.png`) });
  let profile, server;
  try {
    const workspace = await api('POST', '/api/workspaces', { name: 'ProofLoop isolated review', description: 'CONTROLLED FIXTURE' });
    server = await api('POST', '/api/plugins/mcp', { name: 'PROOFLOOP CONTROLLED MCP', transport: 'streamable_http', url: `http://127.0.0.1:${mcp.address().port}/mcp` });
    await api('POST', '/api/capabilities/refresh');
    fixture.caps = (await api('GET', '/api/capabilities?provider=mcp&limit=200')).capabilities;
    assert.equal(fixture.caps.filter((item) => item.kind === 'mcp_tool').length, 2);
    profile = await api('POST', '/api/llm/models', { provider: 'custom', label: 'ProofLoop controlled fixture', model: 'proofloop-loopback', base_url: `http://127.0.0.1:${model.address().port}`, api_format: 'openai', api_key: 'local-test-placeholder', api_key_env: '', temperature: 0, max_tokens: 2048, invoke_timeout_ms: 10000 }, 201);
    await api('POST', `/api/llm/models/${profile.id}/verify`); await api('POST', `/api/llm/models/${profile.id}/activate`);
    await page.goto(`http://127.0.0.1:${frontendPort}/tasks`, { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: '证行目标编译', exact: true }).click();
    await page.getByRole('heading', { name: '从目标生成可验证任务' }).waitFor();
    await page.locator('textarea').fill('收集受控资料，生成可追溯报告，并通过成果及人工复核');
    await page.getByRole('button', { name: '清空', exact: true }).click();
    const beforeEmpty = modelCalls.length;
    await page.getByRole('button', { name: '生成任务计划', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: '没有可用' }).waitFor();
    assert.equal(modelCalls.length, beforeEmpty, 'empty selection cannot expand scope or call the model');
    steps.push('empty_capability_selection_rejected');
    await page.getByRole('button', { name: '全选', exact: true }).click();
    await page.getByRole('button', { name: '生成任务计划', exact: true }).click();
    await page.getByRole('heading', { name: '任务候选计划评审' }).waitFor(); await capture('01-real-plan-review');
    const drafts = (await api('GET', '/api/task-world/proofloop/drafts')).drafts;
    assert.equal(drafts.length, 1); const draft = await api('GET', `/api/task-world/proofloop/drafts/${drafts[0].draft_id}`);
    assert.equal(draft.compilation.status, 'ready');
    await page.getByRole('button', { name: '确认并保存到画布', exact: true }).click(); await page.waitForURL(/\/task-world\/[^/]+$/);
    const graphId = decodeURIComponent(page.url().split('/task-world/')[1]); const path = `/api/task-world/graphs/${graphId}`;
    assert.notEqual(graphId, draft.contract.goal_id);
    const binding = await api('GET', `/api/task-world/proofloop/graphs/${graphId}/goal`); assert.equal(binding.goal_id, draft.contract.goal_id);
    await page.reload({ waitUntil: 'domcontentloaded' }); await page.locator('.react-flow__node[data-id="collect"]').waitFor();
    const graph = (await api('GET', path)).graph; assert.equal(graph.nodes[1].input.capability_bindings[0].source.path[0], 'structured');
    const geometry = await page.locator('.studio-node').evaluateAll((nodes) => nodes.map((node) => ({ width: node.offsetWidth, height: node.offsetHeight })));
    for (const node of geometry) { assert.equal(node.width, 240); assert.equal(node.height, 128); }
    const detail = async () => (await api('GET', `${path}/detail`)).detail;
    const execution = (node, status) => until(async () => (await detail()).nodes.find((item) => item.id === node)?.latest_execution, (item) => item?.status === status, `${node} ${status}`);
    const approval = (node) => until(async () => (await api('GET', '/api/approvals/pending')).approvals.find((item) => item.task_node_id === node), Boolean, `approval ${node}`);
    const runNode = async (node) => { await page.locator(`.react-flow__node[data-id="${node}"]`).click(); await page.getByRole('button', { name: '运行选中节点', exact: true }).click(); await execution(node, 'waiting_approval'); return approval(node); };
    const approve = async (pending, disable = false) => {
      const screen = await page.context().newPage();
      try {
        await screen.addInitScript(() => window.sessionStorage.setItem('yilian.setup.deferred', 'true'));
        await screen.goto(`http://127.0.0.1:${frontendPort}/system?card=approvals`, { waitUntil: 'domcontentloaded' });
        const card = screen.locator(`[data-testid="task-canvas-approval"][data-approval-id="${pending.approval_id}"]`); await card.waitFor();
        if (disable) { available = false; await api('POST', `/api/plugins/mcp/${server.id}/test`); }
        await card.getByRole('button', { name: '允许此次工具操作', exact: true }).click();
        await until(async () => (await api('GET', '/api/approvals/pending')).approvals, (items) => !items.some((item) => item.approval_id === pending.approval_id), 'approval consumed');
      } catch (error) {
        await screen.screenshot({path:join(evidenceDir,'approval-failure.png'),fullPage:true});
        await writeFile(join(evidenceDir,'approval-failure.txt'),`${screen.url()}\n${await screen.locator('body').innerText()}`);
        throw error;
      } finally { await screen.close(); }
    };
    await approve(await runNode('collect')); await execution('collect', 'succeeded'); assert.equal(calls.length, 1);
    const pendingReport = await runNode('report'); await approve(pendingReport, true);
    const failed = await execution('report', 'failed'); assert.equal(failed.failure_code, 'provider_unavailable'); assert.equal(calls.length, 1);
    available = true; await api('POST', `/api/plugins/mcp/${server.id}/test`); await api('POST', '/api/capabilities/refresh');
    await page.getByRole('button', { name: '证行验收', exact: true }).click();
    await page.getByText('目标未达成', { exact: true }).waitFor();
    await page.getByRole('button', { name: '查看修复建议', exact: true }).click();
    await page.getByRole('heading', { name: '受控局部修复审查' }).waitFor(); await capture('02-evidence-based-repair');
    await page.getByRole('button', { name: '确认并应用局部修复', exact: true }).click();
    const repairedApproval = await approval('report'); assert.notEqual(repairedApproval.node_execution_id, failed.execution_id);
    await approve(repairedApproval); const repaired = await execution('report', 'succeeded'); assert.equal(repaired.attempt, 2);
    assert.equal(calls.length, 2); assert.deepEqual(calls[1].arguments.sources, [{ uri, retrieved_at_ms: calls[1].arguments.sources[0].retrieved_at_ms }]);
    steps.push('actual_typed_mcp_chain_and_approved_repair');
    await api('POST', '/api/artifacts/from-execution', { source: { kind: 'node', graph_id: graphId, node_id: 'report', execution_id: repaired.execution_id, workspace_id: workspace.id }, name: 'proofloop-report.md' }, 201);
    await page.getByRole('button', { name: '刷新', exact: true }).click();
    await page.getByRole('button', { name: '进行人工审核判断', exact: true }).waitFor();
    await page.getByRole('button', { name: '进行人工审核判断', exact: true }).click();
    await page.getByPlaceholder('填写审核依据或说明（可选）').fill('已读取隔离成果，来源及摘要核对一致');
    await page.getByRole('button', { name: '提交审核结论', exact: true }).click(); await page.getByText('目标已达成', { exact: true }).waitFor();
    await capture('03-real-artifact-acceptance');
    const reports = (await api('GET', `/api/task-world/proofloop/goals/${binding.goal_id}/evaluations`)).reports;
    assert.equal(reports[0].status, 'pass');
    const evidence = await api('GET', `/api/task-world/proofloop/evaluations/${reports[0].report_id}`);
    const artifact = evidence.snapshot.deliverables[0].artifact_id;
    const preview = await api('GET', `/api/artifacts/${artifact}/preview`); assert.ok(JSON.stringify(preview).includes(uri));
    await page.reload({ waitUntil: 'domcontentloaded' }); await page.getByRole('button', { name: '证行验收', exact: true }).click(); await page.getByText('目标已达成', { exact: true }).waitFor();
    await page.getByRole('button', { name: '刷新', exact: true }).click(); await page.getByText('目标已达成', { exact: true }).waitFor();
    const result = { status: 'passed', stage: 'proofloop_controlled_product_flow', llm: 'LOCAL OPENAI-COMPATIBLE FIXTURE', mcp: 'LOCAL MCP FIXTURE', external_business_benchmark: 'NOT_TESTED', steps, model_requests: modelCalls.length, remote_calls: calls, requests, graph_id: graphId, goal_id: binding.goal_id, graph_revision: reports[0].graph_revision, report: reports[0], geometry, expected_browser_422_errors: 1 };
    await writeFile(join(evidenceDir, 'proofloop-e2e-result.json'), JSON.stringify(result, null, 2)); return result;
  } finally {
    if (profile) await api('DELETE', `/api/llm/models/${profile.id}`, undefined, 204).catch(() => {});
    await Promise.all([new Promise((resolve) => mcp.close(resolve)), new Promise((resolve) => model.close(resolve))]);
  }
}
