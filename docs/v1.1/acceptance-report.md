# YiLianQianYan v1.1 RC1 Acceptance Report

日期：2026-10-06

版本：`1.1.0-rc.1`

公共分支：`feat/v1-1-user-cloud-experience`

冻结基线：`v1.1/base @ 012dc010ebb730148a3583200f8275dcf9a0fa03`

私有仓：`F:\项目开发\忆涟千言\YiLianCloud-Private`，本地 Git，`git remote -v` 为空。

## Verdict

P0 至 P8 的代码、界面、契约、私有服务、Admin、Worker、文档和本地发布工件已经完成。P9 自动化门禁通过。当前候选版本状态为：

```text
Implementation: COMPLETE
Automated gates: PASS
Real AnySearch anonymous smoke: PASS
Windows NSIS build: PASS
Private release artifacts: PASS
Real PostgreSQL service E2E: HUMAN_PENDING
Server deployment and restore drill: HUMAN_PENDING
Installer manual launch and uninstall: HUMAN_PENDING
Code signing: UNSIGNED
Overall v1.1 RC: HUMAN_PENDING
```

不使用编译成功替代真实 PostgreSQL、服务器或安装体验验收，因此尚不标记正式发布。

## Delivered behavior

- Optional Identity：稳定 LocalIdentity、Guest、Authenticated、Offline Authenticated、Closing、设备 Session、refresh token 轮换与单设备撤销。
- Incremental Sync：显式历史上传选择、Outbox、cursor、revision、tombstone、pull restore、durable conflict 和本地/云端显式解决。
- Data minimization：对话与 Message 不同步工具参数/结果；同步 payload 在客户端和服务端递归拒绝 Secret 字段。
- Privacy lifecycle：数据摘要、ExportJob、DeletionJob、会话撤销、用户 tombstone、对象文件清理与最小 Admin audit。
- Setup Wizard：身份、模型、Search、Voice 与完成五步；旧首次 Provider 弹窗已移除。
- AI Search：AnySearch 匿名/BYOK、一级 Search UI、Chat/Task/Knowledge 流转，以及安全网关管理的 `ai_search` Agent 工具。
- Private Admin：用户、同步、存储、生命周期、审计和健康 metadata；无正文、Memory、文件、MCP 参数或 Secret 查看接口。
- Public/Private Boundary：公共 OpenAPI 16 个路径且 Admin 路径为 0；私有实现位于独立 Git Root。

## Automated validation

### Public desktop repository

- Frontend: 106 test files, 505 tests passed.
- Frontend production build: PASS, 2162 modules transformed, Vite build 7.78s.
- Rust: formatting PASS, `cargo check -p yilian-backend` PASS.
- Rust tests: 23 test binaries, 1036 tests passed.
- Architecture boundary: PASS;所有新增 Rust 文件均低于 600 行。
- Tauri Rust check: PASS.
- Public/private scanner: `PUBLIC_PRIVATE_BOUNDARY_OK`, 935 tracked files.
- OpenAPI: 3.1.0, 16 paths, 16 schemas, 0 Admin paths.

### Browser and responsive evidence

使用真实本地后端、system Edge `154.0.4258.53`：

- Setup Wizard 1280×720 截图通过。
- Account 与 Search 覆盖 1280×720、1366×768、1920×1080、2560×1440。
- 8 个页面/尺寸组合均无水平 overflow。
- Account 表单最小字号 16px；Search 表单最小字号 14px。
- E2E 首轮发现 Search 区域选择器为 13px，修复后完整重跑通过。

### Private repository

- `cargo fmt --check`: PASS.
- `cargo check --workspace`: PASS.
- `cargo test --workspace`: 4 contract tests passed.
- Admin Web: 1 test passed; Vite 8.3.3 production build PASS, 1497 modules transformed.
- Admin Web `npm audit --audit-level=moderate`: 0 vulnerabilities.
- Private release boundary: PASS, 47 tracked files; remote list empty.
- Deployment compose parses with PostgreSQL, Cloud API, Admin API, Worker and Nginx gateway services.
- Private release build: PASS; Cloud API、Admin API、Worker、Admin Web dist、migration 与 deployment manifest 均已生成并写入 SHA256SUMS。

### External and release artifacts

- AnySearch anonymous real smoke: HTTP 200, code 0, 3 results, 4451ms; response content was not persisted.
- Windows x64 NSIS: `忆涟千言_1.1.0-rc.1_x64-setup.exe`.
- Installer size: 9,112,793 bytes.
- SHA-256: `CCDEE7B4EC3DD9F4D52713F19E3F072FC40BD6DC7C54EBA4BB89093573F194FA`.
- Signing: unsigned.

Private release binaries:

- `yilian-cloud-api.exe`: 8,146,944 bytes, SHA-256 `70a8ab29002c34b356673261f97ca68330dd108aa48168fdfad3e93edbf9015f`.
- `yilian-admin-api.exe`: 7,011,328 bytes, SHA-256 `d1b5c3f5202a85d410b34eb220f5384ab306ac9eb6966d001e34c14ef610875a`.
- `yilian-cloud-workers.exe`: 6,234,624 bytes, SHA-256 `48ddd36066716e226f35484e16e6e0f3cfac27f4f3f490fce1a4305a52b8a1f7`.

## Evidence locations

Public ignored evidence: `target/v1.1-rc1/`.

- `frontend-tests.txt`
- `frontend-build.txt`
- `backend-check.txt`
- `backend-tests.txt`
- `cargo-fmt-check.txt`
- `tauri-check.txt`
- `public-boundary.txt`
- `openapi-check.txt`
- `anysearch-smoke.json`
- `browser/result.json` and nine PNG screenshots
- `windows-package.txt`

Private ignored evidence: `F:\项目开发\忆涟千言\YiLianCloud-Private\target\v1.1-rc1\`.

Private release artifacts and checksum manifest: `F:\项目开发\忆涟千言\YiLianCloud-Private\artifacts\release\`.

## Remaining RC gates

- 本机没有 Docker、PostgreSQL 服务或 `psql`，因此未执行真实 PostgreSQL migration、Identity/Sync/Deletion/Admin E2E。
- 未把私有服务部署到真实 TLS/VPN/对象存储环境，未执行备份恢复演练。
- NSIS 安装包已构建和哈希，但未在隔离 Windows 用户环境人工安装、启动、升级、卸载。
- 未配置 Windows 代码签名证书；安装包为 unsigned。
- AnySearch 真实 smoke 只验证匿名搜索；用户 BYOK 账号额度和 Cloud Search Proxy 的生产密钥未验证。
