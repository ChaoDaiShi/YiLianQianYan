# 编号展示与 Git 交付补充记录

日期：2026-10-13。用户选择“类型前缀＋短码”，并授权将本轮全部修复合并到 develop、推送 origin。

## 界面行为

- 移除任务工作台顶部“发布 · 未接入”和侧栏“发布动作 · 未接入”，清理相应样式。
- 使用共享 Identifier 组件展示编号，产物 ART、任务图 TG、任务 TK、节点 ND、执行 EX、目标 GL、验收报告 RP、证据 EV、修复 FX、检查点 CP、资源 RS、交付物 DL、条件 CR、工作流 WF、团队 AT。
- UUID 使用前八位大写十六进制字符，旧文字编号使用稳定散列。短码仅用于展示，完整编号用于数据库、请求、恢复、删除和关联操作。
- 可展开完整编号并复制原始值；图卡片等父级按钮内只展示短码，避免嵌套交互控件。
- 验收条件优先使用契约中的说明；说明文本只转换已知且完整匹配的编号，SHA-256 摘要显示短码并保留完整值。
- 证行引擎产物下载使用 ART-短码.md 文件名，下载请求继续使用原始产物编号。

核心新增文件：frontend/src/utils/identifiers.ts、frontend/src/utils/identifiers.test.ts、frontend/src/components/Identifier.tsx。任务工作台、成果面板、任务中心、审批、验收与修复面板已接入。

## 验证范围

界面修改后的生产构建通过，保留已有大 chunk 提示。针对性前端回归共五个文件、17 个不同用例通过。受控浏览器流程验证发布入口消失、TG/ART 短码、完整值展开、复制参数和实际下载文件名；使用真实客户端 API、SQLite 和文件物化，本地模型/MCP 夹具明确标注为受控测试。复制测试隔离 navigator.clipboard.writeText，不写入系统剪贴板，不代表原生桌面剪贴板验收。

本地原始证据目录：target/proofloop-evidence/2026-10-13T02-03-48-479Z。result.json 与 proofloop-e2e-result.json 均记录 passed；最终截图为 proofloop-03-real-artifact-acceptance.png。target 为忽略目录，原始运行证据保留本地，不纳入源码提交。

Git 交付前再次运行同一组 17 个前端用例、node --test scripts/proofloop/evidence-regression.test.mjs、cargo fmt --all --check，均通过。没有重跑完整工作区测试或浏览器流程。可靠性验证记录的 45 个源码摘要中 42 个仍一致；三个后来调整的界面文件为 TaskWorldPage、AcceptancePanel、PlanReviewPanel，已由上述界面回归、生产构建与受控浏览器流程覆盖。后端及云同步源码仍与原可靠性验证一致。

## 交付范围

交付包含六类可靠性修复、云同步三态接线、移除发布按钮、统一编号展示、必要回归及并行开发文档。以 ae586700 为基线，从 fix/proofloop-reliability 按可靠性、云同步、界面和文档分别提交，随后合并到 develop 并推送 origin。实际合并与推送结果以 Git 日志和远端 develop 引用为准；不修改 main，不推送安装包，不部署私有云服务。

本记录不更改专项 ACCEPTANCE_INCOMPLETE 结论，Git 交付不能代替外部业务实验、真实桌面生命周期、两设备验证或独立人员终审。
