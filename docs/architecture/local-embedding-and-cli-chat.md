# 本地向量与 CLI 对话

## 本地向量服务

向量配置入口为「设置 → 向量模型」。轻量中文预设为 `BAAI/bge-small-zh-v1.5`，计算在 CPU 完成，返回 512 维向量。多语预设为 `Qwen/Qwen3-Embedding-0.6B`。两种预设都指向本机 `http://127.0.0.1:8765/v1`，需要先准备模型文件并启动服务。

可使用独立 Python 环境准备 BGE。下载完成后，运行阶段从指定目录加载权重，可关闭网络。

```powershell
python -m pip install torch "transformers==4.57.6" "huggingface_hub==0.36.2"
hf download BAAI/bge-small-zh-v1.5 --revision 7999e1d3359715c523056ef9478215996d62a620 --include "*.json" "*.txt" "model.safetensors" --local-dir .local-models/bge-small-zh-v1.5
python scripts/local-memory-embedding.py --model-dir .local-models/bge-small-zh-v1.5
```

切换预设后保存设置，再测试连接。使用其他本地 OpenAI 兼容向量服务时可直接填写模型标识和监听地址。本机 HTTP 请求允许空凭据，向量请求不经代理或 HTTP 重定向。服务取同一模型目录内的权重和分词配置，权重变化时应使用新模型标识或新地址区分索引。

记忆页的「更多 → 重建索引」可查看已有批次的状态，也可暂停后继续。SQLite 中有效且匹配当前模型的向量保留，后台逐条补齐缺失或过期的向量。应用启动和本机向量设置变更后会检查待索引记录。向量服务不可用时可继续用关键词检索。原文、版本和来源仍由记忆库保存。

迁移 1131 为已有记忆回填 SQLite FTS5 全文索引，并在新增、修订和删除时更新索引。中文短语结合已有的相邻字符匹配。混合检索在作用域、时效和类别条件下融合关键词和向量得分，派生向量仍由 SQLite 原文记录重建。

可使用标注样例核验真实 BGE 的关键词与混合检索。样例包含同义改写、中英文技术文本、项目作用域、更新和撤回。

```powershell
$env:YILIAN_MEMORY_QA_ENDPOINT = "http://127.0.0.1:8765/v1"
cargo test --offline -p yilian-backend --test local_memory_quality -- --ignored --nocapture
```

## CLI 模型

普通聊天在模型菜单中提供 Codex CLI、Codex App Server 与 Gemini CLI。应用在同一条对话中读取本地用户消息、可展示的回复和工具结果。切换到 CLI 时会按预算装配当前上下文，切回 API 后继续读取同一对话。迁移 1130 在本地 SQLite 中保存 CLI 轮次的运行结果、外部会话标识、可用用量和上下文指纹。

- 本地 Codex 从应用进程的 `PATH` 查找，或设置 `YILIAN_CODEX_EXECUTABLE` 为可执行文件的绝对路径。适配器使用已有 Codex 登录，执行 `codex exec` 的 JSONL 模式。
- Codex App Server 使用同一可执行文件，通过标准输入输出发送 JSON-RPC 请求。将 `YILIAN_CODEX_APP_SERVER_HOME` 设为专用凭据目录的绝对路径，在该目录完成 Codex 原生登录。此模式使用独立目录与临时只读线程，按消息增量显示回复。专用目录无需提供 `config.toml`。
- Gemini 需要在应用启动环境中设置 `YILIAN_GEMINI_ENTRY` 为已安装 Gemini CLI 的 JavaScript 入口绝对路径。Node.js 也可通过 `YILIAN_NODE_EXECUTABLE` 指定。使用 `GEMINI_API_KEY` 或 `GOOGLE_API_KEY` 提供原生鉴权，模型从原生流事件读取回复。前台登录状态不被复制到独立的 CLI 工作目录。

CLI 传输目前面向文本对话。工具执行、目标、工作流和附件由既有 API Agent 通道处理。CLI 取消后会回收子进程，在下一轮由本地历史重新构建上下文。

## 本地生成模型

在对话模型配置中选择「llama.cpp（本地）」，填入 `llama-server` 的 OpenAI 兼容地址和实际模型别名。正常聊天工作台使用同一个流式 API。上下文窗口应覆盖工作台提示和随轮资料，例如 Qwen2.5-0.5B-Instruct 的 32K 上下文可承载默认聊天提示。端口地址示例为 `http://127.0.0.1:8080/v1`。

```powershell
$env:YILIAN_LOCAL_CHAT_QA_ENDPOINT = "http://127.0.0.1:8080/v1"
$env:YILIAN_LOCAL_CHAT_QA_MODEL = "yilian-local-qa"
cargo test --offline -p yilian-backend --test cli_chat_switch local_cpu -- --ignored --nocapture
```

安装并登录 Codex 后，可以运行真实 CLI 切换测试。测试使用固定的合成标记和本地 OpenAI 兼容服务夹具。

```powershell
cargo test --offline -p yilian-backend --test cli_chat_switch -- --ignored --nocapture
```
