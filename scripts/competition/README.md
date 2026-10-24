# 自适应工作环境：轻量桌面实测

此脚本调用项目正式 `/api/chat`、SSE、审批续接、工具注册和模型接口，由真实模型决定工具调用，再通过 Windows UI Automation 和文件字节独立判断结果。它不伪造模型回复，也不通过测试脚本直接写出最终简报。

## 当前结果

2026-10-25 的第一次实际桌面执行 **FAIL**。模型调用与资料更新读取真实发生，但 Notepad 恢复旧标签页，原测试只检查进程及字符串，未确认文档归属，导致非测试文档被误修改。预期输出文件未生成，最后由重复工具调用保护停止。完整结果见 [脱敏结果](../../docs/competition/adaptive-work-environment/lightweight-test-result.json)。

发现后，脚本已增加独立空文件、显式带文件路径的快捷方式，以及每次键盘审批前对当前文档与焦点控件的核对。范围不匹配时不读取正文、不截屏、不批准输入。修订后的脚本仅通过语法与回归单测，**未重新执行真实桌面测试，不能把修订当作已经修复产品行为**。

## 环境与边界

- Windows 交互桌面、系统记事本、PowerShell、Python、Node，以及已构建的 `target/debug/yilian-server.exe`。
- 用户现有配置中必须有真实活动模型与 OS 密钥引用。`seed_model_profile.py` 只读源数据库，把一个模型的元数据复制到新的隔离数据库；密钥仍由系统凭据库解析，不导出密钥值。
- 测试限制每个响应 2048 output tokens、16 次工具开始事件；实际模型 usage 单独记录。外部模型调用可能计费，脚本不声称硬性总 token/费用上限。
- 使用新建的仓库外目录。应用数据库隔离不等于桌面文档隔离；必须同时通过窗口、文件身份与焦点控件检查。
- 审批通过正式 API 由本任务授权的测试驱动器处理，仅允许专用记事本快捷方式、限定的文本输入及 Ctrl+A/Ctrl+S。它不是人工审批测试，不修改产品权限策略。
- 来源模型行仍引用原系统凭据，**不要用删除模型的 API 清理这份隔离数据库**，以免连带删除同一个凭据引用。
- 用户已明确要求停止该日志的恢复；文件未完整恢复，本轮不再处理该日志或重复桌面场景。后续独立复验仍须先完成文档身份与控件验证要求。

## 可重复的非桌面检查

```powershell
node --check scripts/competition/desktop_adaptation_smoke.mjs
node --test scripts/competition/smoke_contract.test.mjs
python -c "import ast,pathlib; ast.parse(pathlib.Path('scripts/competition/seed_model_profile.py').read_text(encoding='utf-8'))"
```

## 后续桌面复验命令

以下命令会真正打开记事本和操作键盘；仅供上述前提满足后的独立复验。当前不应据此宣称已通过。

```powershell
$env:YILIAN_SMOKE_DIR = 'F:\tmp\yilian-adaptive-new-run'
$env:YILIAN_SMOKE_PORT = '9448'
# 可选：已配置模型的正式数据目录；默认使用 APPDATA/yilianqianyan-v1
# $env:YILIAN_SMOKE_SOURCE_DATA = '实际数据目录'
node scripts/competition/desktop_adaptation_smoke.mjs
```

场景为：读取绑定知识与 `current-task.json` v1 → 由 Agent 打开指定测试文档并输入草稿 → 测试驱动器修改当前任务 v2 并发送明确通知 → Agent 重新读取、修订、通过编辑器保存 → 独立核对窗口文字、目标文件、过期信息移除和 SHA-256。

这个设计检验“显式变化通知后的适应”，没有证明后台自动感知环境，也没有证明多智能体或跨软件的全局最优组织。环境事件到自主重规划的产品开发和量化对照实验仍在开发报告的后续任务中。
