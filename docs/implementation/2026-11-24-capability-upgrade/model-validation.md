# 模型、CLI 与记忆验收

环境为 Windows。验证使用独立临时目录、合成对话和合成记忆。实施分支为 `feat/personal-proactive-assistant`，基线提交为 `2817d110`。

## 交付测试二进制复验

最终测试程序分别执行三项带真实运行时的用例，均运行 1 项、通过 1 项、退出码 0。API → 真实 Codex 切换用时 12.25 秒，持久化回执记录 17,798 个输入 Token 和 10 个输出 Token。Qwen2.5-0.5B 的普通应用对话用时 61.49 秒，服务端在 32,768 的实际窗口中预填充 8,895 个 Token，生成 10 个 Token。该次应用预算仍使用设置的 200,000，回执中的应用规划值不能用来推断服务端窗口。真实 BGE 质量测试用时 1.23 秒。两个临时本地模型服务已关闭。

现有传输回归 3 项通过，验证同源 307 可以重放嵌入 POST 和鉴权，跨源端点收到零个请求。API 与 CLI 上下文的两个定向测试通过。预算缓存只保留数值估算，按档案、会话、分支、模型、权限和资料指纹区分；历史修改后原回执与不匹配的摘要会失效。

## 本地对话模型

使用 llama.cpp `b12651` Windows CPU 发行版，加载 Qwen2.5-0.5B-Instruct Q4_K_M 权重（模型仓库修订 `9217f5db79a29953eb74d5343926648285ec7e67`）。运行时绑定本机回环地址，无 API 密钥，使用 32K 窗口。 [llama.cpp 发行版](https://github.com/ggml-org/llama.cpp/releases/tag/b12651)，[Qwen 权重与模型说明](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)

使用最新后端二进制的独立副本，通过受保护的 `PUT /api/settings` 保存新提供者 `llamacpp`，随后 `GET /api/chat/readiness` 显示可用。通过正常 `POST /api/chat` 发送 `Say hello in one brief sentence.`，收到 9 个增量事件和最终回复 `Hello! How can I assist you today?`，再从 `GET /api/conversations/:id` 读回已保存的两条消息。

这是一次冷启动样本。llama.cpp 在约 2.25 秒后开始监听，8,842 个提示 Token 的处理用时 88.61 秒，10 个输出 Token 的生成用时 1.21 秒。先前的 8K SmolLM2 模型在普通聊天遇到 8,746 Token 的上下文超限。32K 样本说明默认聊天提示对小上下文模型仍有实际容量要求。

## CLI

- 在 Codex CLI `0.166.0` 的现有 ChatGPT 登录环境中，按生产参数运行 `codex exec --json`，送入合成提示。观察到 `thread.started`、可展示回复 `YILIAN_CLI_READY` 和 `turn.completed`。CLI 报告 19,152 个输入 Token 和 10 个输出 Token。 [Codex JSONL 协议](https://learn.chatgpt.com/docs/non-interactive-mode)
- 使用同一真实 Codex App Server 二进制，在独立空凭据目录中完成 stdio `initialize`，新建临时只读线程，再使用本机 llama.cpp Responses 端点发送合成问候。收到 `item/agentMessage/delta`、最终文本和 `turn/completed`。验证探针通过启动参数指定本地测试模型。产品入口的独立 ChatGPT 登录与账号权限需在对应安装环境验证。 [Codex App Server 协议](https://learn.chatgpt.com/docs/app-server)
- Gemini Headless 适配器按 JSONL 事件规范接入。当前机器没有用于此验收的已鉴权 Gemini CLI 环境。 [Gemini CLI Headless 协议](https://geminicli.com/docs/cli/headless/)

同一会话切换的实时集成测试 `openai_compatible_then_codex_reuses_canonical_history` 已通过。它先从合成的 OpenAI 兼容服务收到标记 `ORANGE-COMET-537`，再让本机 Codex 读取同一对话的前文并返回标记。测试核对了两轮消息及 SQLite 中的 CLI 提供者、完成状态、外部会话标识和上下文条数。同批指令曾一并选中需要单独配置本机生成端点的测试，后者因缺少该环境变量退出。

使用受控 Gemini JSONL 子进程夹具验证撤权。先收到部分回复，再通过 `PATCH /api/v1/profile` 撤回外部模型授权。正在运行的轮次记录为 `cancelled`。恢复授权后，在同一会话发起新轮次，夹具从本地历史读到之前的用户输入，新轮次记录为 `completed`。

## 本地记忆

使用 `BAAI/bge-small-zh-v1.5` 的固定权重修订 `7999e1d3359715c523056ef9478215996d62a620`，运行阶段只从本机目录加载。回环请求无凭据，实测返回 512 维向量。 [BGE 模型卡](https://huggingface.co/BAAI/bge-small-zh-v1.5)

使用真实向量服务、SQLite 和仓库检索代码运行标注样例。关键词 Top-1 为 **0/4**，混合检索 Top-1 为 **4/4**。另两条负例验证了项目作用域隔离和撤回后不可检索。修订旧值后，旧版本向量写回被拒绝。测试样例覆盖同义查询、中英文技术文本、项目作用域和版本纠正。该样本提供回归基线，尚无代表生产质量的规模化检索评测。

对迁移 1131 单独验证了 FTS5 从旧表回填、修订后移除旧检索词，以及删除后清除索引匹配。当前向量仍使用 SQLite 中的派生数据，尚未接入 sqlite-vec。

同一真实 BGE 服务还通过隔离后端的受保护 HTTP 验收。保存回环地址后，`POST /api/providers/embedding/verify` 返回 512 维。新记忆的后台索引状态变为 `completed`，写入一条向量，`GET /api/memories/retrieve` 通过混合检索命中该条记忆。该二进制启动后的临时数据库包含 FTS5 表。

停止隔离后端后，清空测试库唯一记忆的派生向量，再重启同一配置。后台索引自动写回一条向量，SQLite 记录的记忆 ID 和原文与清空向量前一致。

## 可重跑验证

```powershell
$env:YILIAN_MEMORY_QA_ENDPOINT = "http://127.0.0.1:8765/v1"
cargo test --offline -p yilian-backend --test local_memory_quality -- --ignored --nocapture

$env:YILIAN_LOCAL_CHAT_QA_ENDPOINT = "http://127.0.0.1:8080/v1"
$env:YILIAN_LOCAL_CHAT_QA_MODEL = "yilian-local-qa"
cargo test --offline -p yilian-backend --test cli_chat_switch local_cpu -- --ignored --nocapture

cargo test --offline -p yilian-backend --test cli_chat_switch openai_compatible -- --ignored --nocapture
```
