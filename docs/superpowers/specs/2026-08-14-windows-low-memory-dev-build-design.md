# Windows 低内存开发构建设计

## 目标

将 v0.3.1 定义为一次构建稳定性维护：在 Windows 剩余物理内存较少时，降低 Rust/Tauri 开发构建的并行内存峰值，使 `npm run tauri dev` 能完成 Rust 编译、启动后端并打开桌面窗口。

本次不调整产品版本字段。`v0.3.1` 是维护里程碑名称，不修改 `src-tauri/Cargo.toml` 或 `src-tauri/tauri.conf.json` 中现有的 `0.1.0`。

## 已确认的问题

最新故障日志显示：

- Vite 能正常启动；
- Rust 依赖和 `yi-lian-qian-yan` 编译期间出现 `memory allocation failed`；
- `tauri-utils`、`generic-array`、`windows` 等多个 crate 都可能受影响；
- `serde_with` 的 `E0786` 同时伴随 `.rmeta` mmap 失败，应先视为内存耗尽后的次生错误；
- 依赖编译命令已经出现 `strip=debuginfo`，仅继续降低依赖 debuginfo 不能充分限制峰值；
- 当前仓库尚无 `.cargo/config.toml`，Cargo job 数没有仓库级上限；
- 2026-08-14 取证时，Windows 可用物理内存约 4.0 GB，系统提交上限约 31.24 GB，提交余量约 9.3 GB。

因此本轮根因假设是：Cargo/rustc/LLVM 的并行编译与开发 profile 并行代码生成共同提高瞬时提交内存需求，在系统提交余量不足时导致分配失败；`E0786` 不是独立的依赖损坏证据。

## 方案选择

采用仓库级强制低内存配置：

```toml
# .cargo/config.toml
[build]
jobs = 1
```

```toml
# Cargo.toml
[profile.dev]
debug = 0
incremental = false
codegen-units = 1

[profile.dev.package."*"]
debug = 0
codegen-units = 1
```

这会同时限制三个主要来源：

1. `jobs = 1`：同一时间只运行一个 Cargo 编译 job；
2. `codegen-units = 1`：减少单个 crate 内部 LLVM 并行代码生成；
3. `incremental = false` 与 `debug = 0`：减少开发构建的增量状态和调试信息开销。

代价是开发构建速度下降，且开发二进制不包含调试信息。v0.3.1 优先保证资源受限机器可启动；恢复更高并行度属于后续基于实测峰值的独立优化，不在本次范围内。

## 修改边界

只允许：

- 新增 `.cargo/config.toml`；
- 修改根 `Cargo.toml` 的 `[profile.dev]` 与 `[profile.dev.package."*"]`。

禁止：

- 修改 `backend/Cargo.toml`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`；
- 修改任何 Rust、TypeScript、React 或 CSS 业务代码；
- 修改或删除 `Cargo.lock`；
- 执行 `cargo clean`、`cargo update` 或删除 `target`；
- 降级 Rust、Tauri 或 `serde_with`；
- 把 `.claude/` 或其他既有未跟踪内容加入本次提交；
- 修改 Windows 页面文件设置。页面文件仍可作为用户侧环境建议，但不由仓库代码自动操作。

## 配置流

```text
npm run tauri dev
  -> Tauri CLI
  -> beforeDevCommand: frontend Vite
  -> cargo run
  -> repository .cargo/config.toml: jobs=1
  -> workspace Cargo.toml: dev debug=0, incremental=false, codegen-units=1
  -> yi-lian-qian-yan starts backend
  -> Tauri window opens
```

根配置同时覆盖开发者直接执行的 `cargo build -p yi-lian-qian-yan` 和 Tauri CLI 间接执行的 `cargo run`，避免依赖容易遗漏的临时 PowerShell 环境变量。

## 失败处理

若单作业低内存配置下仍发生 OOM，本轮停止继续调整依赖或源码，只收集：

- 首个 OOM crate 和相邻完整日志；
- `rustc --version --verbose` 与 `cargo --version`；
- Windows Available RAM、Committed Bytes、Commit Limit；
- `cargo -vv` 中能够证明实际 job/profile 配置的命令片段。

随后单独判断是单个 rustc 进程峰值、工具链回归、Windows crate feature 膨胀，还是系统提交上限不足。没有新证据前不把 `E0786` 当作依赖损坏处理。

## 测试与验收

配置修改本身不新增生产函数。测试采用“配置断言 + 真实构建 + 真实桌面启动”的分层方式：

1. RED：在 `.cargo/config.toml` 尚不存在、profile 尚未达到目标值时运行配置断言，确认它因 `jobs`、`incremental` 或 `codegen-units` 不符合要求而失败；
2. GREEN：应用最小 TOML 修改后，重新运行同一断言并确认通过；
3. 运行 `cargo build -p yi-lian-qian-yan -j 1`，必须完整成功，不能用 `cargo check` 代替；
4. 运行 `npm.cmd run tauri dev`，确认 Vite ready、Rust 编译完成、后端启动、Tauri 窗口实际出现，且日志中不再出现 `memory allocation failed`；
5. 关闭本次启动的开发进程后执行回归：
   - `cargo fmt --check`；
   - `cargo check --workspace`；
   - `cargo test --workspace`；
   - `npm.cmd test --prefix frontend`；
   - `npm.cmd run build --prefix frontend`；
   - `git diff --check`；
6. 最终审计 `git status --short` 与提交文件列表，确认未包含 `.claude/`、`Cargo.lock` 或业务源码。

桌面窗口实际出现是最终启动验收的一部分；仅源码检查、配置解析、`cargo check` 或前端生产构建均不足以宣称问题已解决。

## 安全影响

本次不改变 Agent、Tool、审批、Sandbox、SSE、API 或数据库行为。`jobs = 1` 只降低本地构建并行度，不扩大运行时权限。仓库不会自动调整虚拟内存、终止用户进程或清理构建缓存。

## 交付边界

实现提交使用 Conventional Commit，建议为：

```text
fix(build): stabilize low-memory Windows dev builds
```

在验证通过前不推送、不合并。即使实现成功，也只报告当前机器、当前工具链和当前提交下的实际验证结果，不将其外推为所有 Windows 设备上的保证。
