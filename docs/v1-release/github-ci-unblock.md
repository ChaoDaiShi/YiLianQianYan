# GitHub CI unblock — v1/release-work

当前 `v1/release-work` 源码已经上传，但现有 GitHub 凭据没有 `workflow` scope，无法提交 workflow 修改。Remote CI 状态为：

```text
BLOCKED_BY_WORKFLOW_SCOPE
```

仓库维护者只需在 GitHub 网页完成以下操作：

1. 打开 `.github/workflows/shared-foundation.yml`。
2. 切换到 `v1/release-work` 分支。
3. 在 `push.branches` 中加入：

   ```yaml
   - "v1/**"
   ```

4. 直接提交到当前分支。
5. 查看由该提交触发的真实 Actions run，并核对 `head_branch=v1/release-work`、`head_sha` 和所有 job 结果。

不要修改 jobs、permissions、Rust toolchain、Node 版本或 Actions 版本。不要把 PAT 写进仓库、`.env` 或聊天。workflow 文件存在不等于 CI 已通过；只有真实 run 全部成功才能记录 `REMOTE CI: PASS`。
