# 自适应工作环境：轻量桌面实测

此脚本调用项目正式 `/api/chat`、SSE、审批续接、工具注册和模型接口，由真实模型决定工具调用，再通过 Windows UI Automation 和文件字节独立判断结果。它不伪造模型回复，也不通过测试脚本直接写出最终简报。

## 当前结果

2026-10-25 的第一次实际桌面执行 **FAIL**。模型调用与资料更新读取真实发生，但 Notepad 恢复旧标签页，原测试只检查进程及字符串，未确认文档归属，导致非测试文档被误修改。预期输出文件未生成，最后由重复工具调用保护停止。完整结果见 [脱敏结果](../../docs/competition/adaptive-work-environment/lightweight-test-result.json)。

发现后，脚本增加独立空文件、显式带文件路径的快捷方式，以及每次键盘审批前对当前文档与焦点控件的核对。范围不匹配时不读取正文、不截屏、不批准输入。

用户后续授权补测，并提示额度约剩3元。补测先在正确文档中复现 Windows 批量 Unicode 输入丢字，随后通过不调用模型的原生对照定位问题，生产文本注入改为逐字符、10ms 间隔。最终真实桌面复验 **PASS**：9 次真实 Flash 请求、7 次工具调用、16.291 秒，窗口全文与保存文件相同，旧日期/地点/标记均已替换。首次失败保持 FAIL。详情见[补测结果](../../docs/competition/adaptive-work-environment/lightweight-test-supplement-2026-10-25.json)与[开发报告第11.4节](../../docs/competition/adaptive-work-environment/development-report.md#114-2026-10-25-补充实测受限桌面流程通过)。

## 环境与边界

- Windows 交互桌面、系统记事本、PowerShell、Python、Node，以及已构建的 `target/debug/yilian-server.exe`。
- 用户现有配置中必须有真实活动模型与 OS 密钥引用。`seed_model_profile.py` 只读源数据库，把一个模型的元数据复制到新的隔离数据库；密钥仍由系统凭据库解析，不导出密钥值。
- 官方 Flash 非思考模式；每次最多512输出 tokens、10次模型请求、12次工具开始事件。转发前按输入 UTF-8 字节数及封装余量、官网较高时段的2元/百万输入和8元/百万输出做保守预留；超预算先拒绝，不重写模型输出。单次运行费用预留上限不超过0.30元，可进一步降低；精确用量、缓存命中和余额接口分别记录。
- 使用新建的仓库外目录。应用数据库隔离不等于桌面文档隔离；必须同时通过窗口、文件身份与焦点控件检查。
- 审批通过正式 API 由本任务授权的测试驱动器处理，仅允许专用记事本快捷方式、限定的文本输入及 Ctrl+A/Ctrl+S。它不是人工审批测试，不修改产品权限策略。
- 来源模型行仍引用原系统凭据，**不要用删除模型的 API 清理这份隔离数据库**，以免连带删除同一个凭据引用。
- 用户已要求停止原日志恢复；后续补测只使用独立测试文件，没有继续处理该日志。

## 可重复的非桌面检查

```powershell
node --check scripts/competition/desktop_adaptation_smoke.mjs
node --test scripts/competition/smoke_contract.test.mjs
node --test scripts/competition/model_cost_guard.test.mjs
python -c "import ast,pathlib; ast.parse(pathlib.Path('scripts/competition/seed_model_profile.py').read_text(encoding='utf-8'))"
```

## 后续桌面复验命令

以下命令会真正打开记事本和操作键盘；使用全新的仓库外目录，并保留文档身份和焦点检查。已通过的是记录中的那个场景，不能据此断言所有桌面任务都已通过。

```powershell
$env:YILIAN_SMOKE_DIR = 'F:\tmp\yilian-adaptive-new-run'
$env:YILIAN_SMOKE_PORT = '9448'
$env:YILIAN_SMOKE_MAX_CNY = '0.30'
# 可选：已配置模型的正式数据目录；默认使用 APPDATA/yilianqianyan-v1
# $env:YILIAN_SMOKE_SOURCE_DATA = '实际数据目录'
node scripts/competition/desktop_adaptation_smoke.mjs
```

场景为：读取绑定知识与 `current-task.json` v1 → 由 Agent 打开指定测试文档并输入草稿 → 测试驱动器修改当前任务 v2 并发送明确通知 → Agent 重新读取、修订、通过编辑器保存 → 独立核对窗口文字、目标文件、过期信息移除和 SHA-256。

这个设计检验“显式变化通知后的适应”，没有证明后台自动感知环境，也没有证明多智能体或跨软件的全局最优组织。环境事件到自主重规划的产品开发和量化对照实验仍在开发报告的后续任务中。

## 无模型费用的输入诊断

`diagnostics/native_input_probe.py` 只用于已有测试运行目录中的独立文档。它核对精确文档名称，再替换该测试文档的当前正文，对比一批输入和10ms分批输入，并通过同一个 UIA 观察器读回；不调用模型、不生成任务最终成果。不要把这个诊断脚本的输入当作智能体执行成功。

```powershell
python scripts/competition/diagnostics/native_input_probe.py 'F:\tmp\某次独立测试目录'
```

本轮两次真实运行总计27,676 tokens，按当日空闲费率及实际缓存用量估算0.01624272元；余额接口显示3.89→3.88元。分位余额显示与费用估算不是同一种精度，均保留在结果中。[官方价格](https://api-docs.deepseek.com/zh-cn/quick_start/pricing/)
