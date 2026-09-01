# Parallel Development

## Common start

AI-A 与 AI-B 必须从同一个不可变标签开始：

```powershell
git fetch origin --tags
git rev-parse 'shared-foundation-v1-v2^{commit}'
```

两者输出必须一致。推荐分支为 `v1/freeform` 与 `v2/desktop`，推荐独立目录为 `YiLian-v1` 与 `YiLian-v2`。禁止两个 Agent 共用同一 working directory。

```powershell
git worktree add ..\YiLian-v1 -b v1/freeform shared-foundation-v1-v2
git worktree add ..\YiLian-v2 -b v2/desktop shared-foundation-v1-v2
```

执行前先确认目标路径不存在且不覆盖用户工作；不要 reset、clean 或移动其他 worktree。

## Rolling integration

建立 `integration/v1-v2`。领域内部提交留在各自分支；以下内容达到可消费状态后尽早进入 integration：新增跨域 Event/Command、Projection 字段、真实 provider adapter、Surface host contract 或 migration。禁止两条线长期隔离后一次性大合并。

Shared contract 默认 additive first。跨域变更应附 contract test 和 maturity 标记。L1 Mock 足以解除另一条线的等待；L2/L3 可以后续替换，Consumer 不应因 Provider 替换而重构。

## Required checks

合入 integration 前至少运行：

```powershell
cargo fmt --check
cargo check --workspace
cargo test --workspace --all-targets
cd frontend
npm.cmd test
npm.cmd run build
```

Windows native/Tauri 改动必须额外运行 `cargo check -p yi-lian-qian-yan` 与适当 GUI smoke。结果要区分 source check、自动化测试、真实运行时和安装包验证。
