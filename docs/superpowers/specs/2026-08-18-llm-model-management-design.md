# 多服务商 LLM 模型管理与 Token 用量设计

## 目标

在设置页增加多个 OpenAI 兼容模型档案的管理能力，支持 OpenAI、DeepSeek、千问、GLM 和自定义服务商；连接验证成功后可激活模型用于聊天，并持久化记录 provider 返回的 token 用量，提供模型树与按时间段的柱状图。

## 方案

采用“兼容旧配置的模型档案层”：现有 `AppConfig.model` 继续作为没有模型档案时的 fallback；新增 SQLite `llm_models` 保存非敏感配置，API Key 使用现有 `SecretStore`，只保存稳定 `SecretRef`。模型档案拥有唯一激活状态，聊天请求开始时读取激活档案并构造现有 `ModelConfig`，因此不会重写 Agent/ReAct 核心。

每次 OpenAI 兼容流式请求声明 `stream_options.include_usage=true`，解析最终 SSE chunk 的 usage，并写入 `llm_usage_events`。不对 provider 未返回的 usage 伪造估算；无 usage 的调用仍完成，但不会污染统计数据。连接验证使用同一客户端发送最小请求，验证成功后也记录真实 usage。

## API

- `GET /api/llm/models`：返回模型档案列表，API Key 仅返回 `api_key_configured` 和 `api_key_source`。
- `POST /api/llm/models`：创建档案，保存 key 到系统凭据库；输入支持 `provider`、`label`、`model`、`base_url`、`api_format`、`api_key`、`api_key_env`、`temperature`、`max_tokens`、`invoke_timeout_ms`。
- `PUT /api/llm/models/:id`：更新档案；空 key 表示保留旧 key，`clear_api_key=true` 才清除。
- `DELETE /api/llm/models/:id`：删除档案及其凭据，激活档案删除后回退旧配置。
- `POST /api/llm/models/:id/verify`：验证连接并返回安全错误信息、验证时间和 usage。
- `POST /api/llm/models/:id/activate`：设置当前聊天使用的模型。
- `GET /api/llm/usage?model_id=&from=&to=`：按模型返回总量和日聚合柱状图数据。

接口受现有 control session 保护，模型数量上限为 32，usage 查询最多返回 366 天范围与 5000 条事件聚合结果。

## 前端体验

“设置 → 模型”分为三块：

1. 服务商/模型档案：预设下拉框自动填充地址，支持新增、编辑、删除、验证和激活；key 始终为密码输入框，已保存 key 不回显。
2. 模型树：根节点为“LLM 模型森林”，每个已添加档案为枝节点，最多布局 32 个节点，使用响应式 SVG、固定 viewBox 和节点截断，避免模型数量把页面撑破。
3. Token 用量：可按模型和开始/结束日期筛选，显示输入、输出、总 token；柱状图按日渲染，最大柱高归一化到容器内，并在无数据时显示空状态。

所有请求失败、验证失败和 key 未配置均在设置页内显示，不将服务端错误原文中的 secret 传到 UI。

## 验证

- Rust：数据库模型 CRUD、SecretRef 不落明文、连接验证、SSE usage 解析与 usage 聚合单元/集成测试；运行 `cargo fmt --check`、`cargo check`、`cargo test`。
- React：模型档案 API 类型、树布局边界、用量图筛选与设置页交互测试；运行 `npm.cmd test`、`npm.cmd run build`。
- API：使用本地 mock OpenAI 服务验证 connection verify、chat streaming usage 入库和激活模型切换。

## 不包含

不改写 Agent 核心循环，不新增 Python/云数据库，不实现各家非 OpenAI 兼容协议的特殊请求体，不展示或导出明文 API Key，不把历史无 usage 的调用伪造成精确 token 数据。
