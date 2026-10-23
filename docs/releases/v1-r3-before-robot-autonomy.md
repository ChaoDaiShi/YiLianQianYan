# R3 基线：机器人自主能力扩展之前

基线标识：`baseline/v1-r3-before-robot-autonomy`。

本记录对应用户要求“先合并一版到 develop，并记录好这一版本”。以该标识定位本次集成后的源码；后续机器人自主能力、额外微信方案和消息工作台重排应在新功能分支开发。

## 源码范围

- 前置提交：`bec18552cbdb8ddb23de0215254b3c3376d7fc22`。
- 应用清单版本维持 `1.1.0-rc.1`；本标识是源码检查点，不代表新安装包或正式发布。
- 包含当前 R3 的对话轮次与输入持久化、上下文压缩、知识构建、记忆版本治理、工作流运行参数、自动化、学习、Git/LSP 协作及资源／成果处理。
- 包含飞书、QQ OneBot 11、微信 iLink 渠道、接入模板与回复预设、自动回复和人工审阅。
- 包含附件卡片与正文预览、对应消息附件持久化、历史图片按实际资源引用和上传轮次恢复位置、桌面／渠道历史分类、会话分支及画布交互。
- 已发送附件位于对应用户文字气泡上方；待发送附件位于输入文字上方。

## 验证与证据边界

- 前端：166 个测试文件、788 项测试通过；生产构建通过。
- 后端：1,228 项库测试与 10 项安全执行集成测试通过；`cargo fmt --all --check` 与 `cargo check -p yilian-backend` 通过。
- 本机默认 TEMP 位于另一个 Git 工作树内；非仓库测试通过 `YILIAN_GIT_TEST_NONREPO_ROOT` 指向已核实的外置目录，未修改生产代码来绕过该测试边界。
- QQ 与微信的真实账号私聊自动收发已由此前桌面验收及用户收件确认；飞书完成受控协议验证，尚无真实飞书账号收发验收。
- 附件与分支已有真实资源接口、浏览器交互和本机 Tauri 旧对话图片位置验收。部分模型／业务流程使用受控服务，不把它们标为真实外部服务或模型质量验收。
- 文档内的原型和预览图属于设计参考；机器日志、数据库、真实账号截图与本机凭据不属于源码交付。
- 本轮版本记录不包含后续 QQ 主动控制、额外微信协议或消息页新布局的实施状态。

## 复验命令

在仓库根目录执行；先将下面的外置目录设为当前机器上已确认不属于任何 Git 工作树的路径。

```powershell
$env:YILIAN_GIT_TEST_NONREPO_ROOT = '<external-nonrepo-test-directory>'
cargo fmt --all --check
cargo check -p yilian-backend
cargo test -p yilian-backend --lib --no-default-features -- --test-threads=1
cargo test -p yilian-backend --no-default-features --test security_execution -- --test-threads=1
npm --prefix frontend test -- --run --maxWorkers=1 --minWorkers=1
npm --prefix frontend run build
```

## 版本定位

```powershell
git show baseline/v1-r3-before-robot-autonomy
git rev-parse 'baseline/v1-r3-before-robot-autonomy^{}'
```

基线后继续保留现有 SQLite 用户资料和本机渠道配置；不要用源码检查点覆盖运行数据库。
