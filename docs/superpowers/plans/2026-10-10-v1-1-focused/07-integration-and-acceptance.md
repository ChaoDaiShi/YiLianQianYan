# P7：集成候选、两设备和安装生命周期验收

> 待执行。P1—P5 交付后可开始 P7a 集成与构建；P6A/B 独立执行，P6C 等候选产物出来后核验运行版本，所有证据在 P7b 汇总。使用 executing-plans；未经用户明确告知，不使用子智能体。

## 集成对象与顺序

CLIENT=`F:\项目开发\忆涟千言\YiLianQianYan`；SERVER=`F:\项目开发\忆涟千言\YiLianCloud-Private`。

P1、P4、P5 的业务实现互相独立，可先进入集成候选。合并 P2/P3 时由一个人处理 cloud.rs、cloud.ts、session.rs、revocation.rs、sync.rs 的函数级补丁；核对二者都使用 P0 先行提交的一个云操作锁。不能出现两把名称不同、互相不共享的锁。

每完成一个工作包的合并，运行其相关测试；所有工作包进入候选后仅运行一轮完整必需检查。只有代码再次变化或某个失败未解释时重跑对应项目，不以重复全量测试代替真实验收。

```powershell
# CLIENT
cargo fmt --check
cargo check --workspace
cargo test --workspace --all-targets
npm.cmd --prefix frontend test -- --run
npm.cmd --prefix frontend run build

# SERVER（另一个工作目录）
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

每条原生命令非零都必须停止并记 FAIL，不能在 PowerShell 中因最后一条成功而覆盖前面失败。不要只保存命令字符串；保存真实退出码、输出和执行 commit。

## 双设备/双账号验收 D01—D05

使用两台实际 Windows 设备，或者两台独立 Windows VM，各自拥有独立数据目录、进程和 SecretStore。两个 `Database` 对象、两个浏览器 tab、一个进程里两个 mock 不属于这里的两设备验收。

预先记录设备 D-A/D-B、客户端 commit 与 installer SHA-256、服务端 commit/制品 hash、环境地址。仅用合成测试账号，不输出密码/令牌。

| 用例 | 操作顺序 | 通过依据 |
|---|---|---|
| D01 退出后切换 | D-A 用 A 登录，创建独有对话/记忆，退出；登录 B；调用旧 switch 输入 A 编号；再次用 A 正确密码登录 | 未认证切换 401；B 看不到 A；认证 A 后原内容仍在 |
| D02 类别关闭/开启 | D-A 关闭会话组；D-B 同账号新增会话及记忆；D-A 同步、重启；再开启会话组同步 | 关闭时记忆同步、会话不恢复；重新开启后会话补齐，无重复 |
| D03 关闭期间两端修改 | D-A 关闭记忆同步并修改旧记忆；D-B 修改/删除同条；D-A 再开启 | 本地修改保留，按原冲突或 tombstone 规则提示；不静默覆盖 |
| D04 离线退出 | D-A 断开云连接后退出，重启访客，再恢复连接；从 D-B 查看会话 | D-A 不显示远端成功；待办可在访客启动后处理；确认后 D-B 看到会话已撤销 |
| D05 数据错误反馈 | 临时测试资料库中使创建/删除/list SQL 失败，操作界面 | 显示错误，未创建虚构记录，旧列表和输入仍在；修复故障后可正常操作 |

D05 的故障仅注入合成测试库，不能通过删除实际用户表验证。端到端录屏/截图同时记录动作及结果；一张最终列表截图不足以证明切换和恢复过程。

## E21：安装、升级、卸载

复用已有 `scripts/build-windows.ps1`、`scripts/verify-release-version.ps1` 和 `scripts/test-windows-installer.ps1`。已有安装 smoke 只静默安装并卸载，没有启动 UI、跨版本升级或用户数据验证，不能单独把 E21 改为 PASS。

候选打包：

```powershell
# 在 CLIENT 根目录；默认使用仓库当前候选版本，不为本计划自行增加版本号。
npm.cmd run build:windows
```

若已在相同源码候选完成本阶段完整检查，可使用已有 `-SkipTests` 避免重复执行；必须附对应检查 commit，不能省略检查。

| 步骤 | 操作 | 必须记录 |
|---|---|---|
| 1 | 干净 Windows VM 安装候选 | NSIS 退出码、应用实际窗口、运行版本、后端连接成功 |
| 2 | 创建访客和测试账号内容，退出并重启 | 内容与账号切换行为符合 P2，普通离线使用正常 |
| 3 | 在另一 VM 安装实际旧发布包并写入样本，再覆盖升级 | 升级路径、迁移前后元数据、样本完整；历史编号未重写 |
| 4 | 退出账号并卸载 | 应用/后端进程结束、安装程序目录移除；保留数据政策明确 |
| 5 | 重装候选 | 保留的已退出资料需重新登录；重新登录可恢复，不误显示已清除所有本地数据 |

保留用户资料是当前卸载策略时，不能以“卸载后目录仍有数据库”判失败；也不能在 UI 中声称这些数据已删除。这里验证当前策略与实际结果一致，不新增“卸载自动清理所有资料”功能。

调用现有 smoke 的参数是 `-InstallerPath`，必须使用本次构建日志中实际输出路径并计算 hash，不猜安装包文件名。E21 只有完整五步都有证据才 PASS。

## E20/E22 交接

E20 由 SERVER 的 `docs/optimization/2026-10-10-server-acceptance.md` 定义。P7 收到结果后检查：真实恢复来源、最新删除记录在旧备份之外、恢复期间服务关闭、注销账号不可重新登录、未注销账号数据完整。

E22 要求实际运行的 cloud-api/workers/admin-api 制品与候选 hash 对应；只读核验无需再次请求授权，部署/重启需要已有明确目标与授权。若当前未获生产部署授权，准备好候选制品与具体部署步骤后，把需要执行的动作列明；不能把 Git push 当作服务升级。

管理前端本轮不做业务修改。若部署包含管理端静态资源，记录其基线和 hash；接口改动的验证通过现有管理 API 做，不要求重做页面。

## 证据汇总和验收脚本调整

现有 `scripts/verify-v11-reliability.ps1` 最后固定将 E21/E22 写为 NOT_TESTED，不能靠手改 result.json 变成 PASS。集成人对脚本做最小调整：保留自动 E01—E20 执行；为 E21/E22 接收独立验收记录文件，在核对 candidate commit、制品 hash、用例状态和证据文件存在后汇总，标记 `source=manual_evidence`，不声称脚本自动完成了手工验收。

采用明确输入 `-InstallerEvidenceFile`、`-DeploymentEvidenceFile`；不提供时仍 NOT_TESTED。提供的文件 commit/hash 不匹配候选则 FAIL。不从文件里执行任何 shell 命令。

E21/E22 记录形状：

```json
{
  "case_id":"E21",
  "source":"manual_evidence",
  "status":"NOT_TESTED",
  "client_commit":"待执行时填写实际提交号",
  "server_commit":"待执行时填写实际提交号",
  "artifact_sha256":"待执行时填写实际产物哈希",
  "evidence_files":[],
  "operator":"执行者",
  "verified_at":null
}
```

上例是未执行结构，禁止用它声明完成；实际 PASS 必须有非空证据文件、时间、执行者及真实 commit/hash。汇总器只检查记录完整性，P8 审查者还须阅读原证据。

新增本轮 D01—D05 和 P1—P5 检查单，不修改 E01—E22 的历史语义。下游输入为 `candidate-evidence.json`，包含两个 commit、全部 case、制品和证据索引。

- [ ] 合并冲突检查及共享字段契约一致。
- [ ] 两设备真实行为通过，不能用接口 mock 代替。
- [ ] E20、E21、E22 分别给出实际状态。
- [ ] 固定最终候选提交及证据索引，交 P8；此时不得写“正式发布已通过”。
