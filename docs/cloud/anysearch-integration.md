# AnySearch Provider Integration

核对日期：2026-10-06

## Public provider contract

YiLianQianYan 的 `SearchProvider` 保持服务商中立，AnySearch 是首个实现。直接搜索调用官方 `POST https://api.anysearch.com/v1/search`，发送 `query`、`max_results`、可选 `tag`、`zone` 和 `language`。响应被归一化为 request ID、标题、URL、摘要、可选正文、结果数和耗时。

官方资料：

- [POST v1 search](https://www.anysearch.com/docs/api-endpoints/v1-search)
- [Authentication](https://www.anysearch.com/docs/auth)
- [Quick Start](https://www.anysearch.com/docs/quick-start)

## Authentication

匿名模式不发送 Authorization；BYOK 模式从本机 SecretStore 读取 Key，并只在请求时设置 Bearer header。无效 Key 返回 401/403 时不会静默降级为匿名模式。公共 UI、SQLite、同步 payload、日志和 Agent 结构化结果均不包含 Key。

AnySearch 的匿名 402 响应可能包含自动生成的凭据，因此适配器不会把原始 message 写入日志、测试证据或错误 UI。产品只报告 `search_quota_exhausted`，提示用户在本机配置 Key。

## Agent boundary

`ai_search` 工具只允许 `https://api.anysearch.com/v1/search`，通过 SecurityExecutionGateway 请求 `network.request` 与精确 POST 目标授权。自定义 Search base URL 仍可用于用户主动操作的 Search Surface，但不会自动成为 Agent 可调用目标。

## Real smoke

匿名 smoke 使用通用技术查询，不包含用户数据。当前证据：HTTP 200、business code 0、3 个结果、request ID `ed53686a-cdd6-42ec-a201-f0eec1242aba`。证据 JSON 不保存响应正文或 message。
