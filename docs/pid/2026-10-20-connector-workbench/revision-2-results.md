# 2026-10-21 第二轮验收修订结果

本轮按用户的“不合格”反馈和后续消息画布位置确认实施；工作分支仍为`codex/connector-workbench`，保留上一轮工作树，未提交、未推送。

## 需求对应

| 用户反馈 | 本轮改动 | 实际验收 |
|---|---|---|
| 对话底层与输入区大面积白底 | 移除根容器页面洗色、执行栏/ContextRail实底、输入区外层渐变；输入框卡片本体也使用透明背景，保留文字和边界 | 浅/深色下逐一检查computed backgroundColor为rgba(0,0,0,0)、backgroundImage为none |
| 提示框改到上方 | CyreneToast移到top:16px/right:16px | 浏览器和实际Tauri顶部位置检查、截图 |
| 消息树才是所需画布 | 连接器左侧为问答节点无限画布：默认竖直主链、选中回答继续提问、同一回答分出多条问题、节点详情/拖动/缩放/选中路径/布局保存 | 真实SSE依次发送“你好”“告诉我你的名字”，再从首个回答分出“你认为什么是善”；六节点、祖先隔离、重开恢复通过 |
| 资源连线应上下左右且为曲线 | 智能体Top约束、Bottom记忆知识、Left对话、Right工具/模型等；对应端点与三次贝塞尔曲线 | 实际DOM四侧端口、SVG路径含C命令，拖线按端口语义校验 |
| 原任务画布归入工作流 | 能力→工作流→任务图工作流内直接打开原Studio，返回同一管理弹窗；复用原ID与停止删除API | 原图打开、返回、停止删除及GET404通过，没有复制或自动转换原业务数据 |
| 使用新透明图标 | 原PNG字节原样用于前端品牌；已安装的Tauri图标工具生成配置中的PNG/ICO/ICNS | 原图和favicon SHA256相同；所有原生资源保留alpha=0..255与透明角像素，桌面构建通过 |
| 连接器画布不能删除 | 增加顶部“删除当前画布”；列表、重命名、确认在同一个Modal切换；异常显示真正的失败信息 | 当前/非当前/最后画布删除、Tab→Enter确认、删除后空状态在浏览器和实际Tauri通过 |

## 数据与执行

新增1023消息分支元数据迁移。每个分支复用普通聊天执行和实际消息行，只复制所选回答的完整祖先协议前缀；兄弟分支不进入该次模型请求。分支运行继续使用所绑定智能体、模型池、用户提示词与权限网关。原对话内容不改写。消息树目录排除内部执行分支副本。

接口位于`backend/src/api/connector_messages.rs`，目录、树、选中、布局及turns端点都在`/api/connectors/:canvas/...`下。前端共享原SSE解析，不建立第二套模型执行器。停止生成沿现有chat/stop，并修正取消结束后不应被覆盖为failed的状态保存。

## 查明并修正的缺陷

- 旧管理弹窗和删除确认弹窗同时捕获Tab，导致确认按钮无法通过键盘到达。单Modal状态切换已消除该问题。隔离检查中旧鼠标路径能发DELETE，因此本轮没有将未复现的原生鼠标原因写成既定事实；实际原生三种删除路径现已通过。
- 控制节点没有回存ReactFlow发出的measured尺寸，导致节点测量状态反复失效，延迟的回答定位覆盖手动fitView。现已保留实际测量数据，六个节点适应视图后全部位于画布内，正常点击与重开视口均通过。
- SSE错误先于持久化结束时不提前刷新；图在stream_end后读取真实保存状态。HTTP错误/中止也通过共享SSE客户端发出终止通知。

## 验证结果

| 范围 | 结果 | 证据 |
|---|---|---|
| 前端 | 140文件/666测试通过；生产构建通过 | target/revision2-vitest.json、target/revision2-build.log |
| 后端 | 1065单元测试、22项相关集成测试通过；cargo check通过 | target/revision2-rust-tests.log、target/revision2-cargo-check.log |
| 浏览器整合 | 15项检查通过，实际后端与Edge | target/canvas-e2e/2026-10-21T02-57-56-101Z/result.json |
| 原生整合 | 原10项浮窗检查及新增第二轮整合流程通过（该流程含同一组15项断言） | F:/target/workbench-desktop/2026-10-21T02-58-06-448Z/results.json |
| 桌面构建 | QA与正常配置均通过 | target/revision2-native-build.log、target/revision2-native-normal.log |
| 图标 | 原PNG与favicon哈希一致，PNG/ICO/ICNS透明通道保持 | verification/revision2-summary.json |

本轮测试使用独立目录、端口和QA进程，没有复制用户配置或读取原有密钥。模型行为验证使用受控HTTP服务经过真实产品调用链；没有进行付费厂商账户与声学设备验收。Vite仍提示主包超过500kB，构建成功。未生成安装包或替换已安装程序。

[实际运行预览](revision2-previews/index.md) · [机器可读结果](verification/revision2-summary.json) · [第二轮实施计划](revision-2-plan.md)
