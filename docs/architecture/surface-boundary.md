# Surface Boundary

`WorkspaceSurface` 是现有 v0.9 工作区 UI 的唯一公共 Surface。`StandaloneHost` 直接承载它；`DesktopSurfaceSkeleton` 也只通过同一个 Surface contract 承载它。

```text
AppRoot
├─ StandaloneHost → WorkspaceSurface
└─ DesktopHostSkeleton → DesktopSurfaceSkeleton → WorkspaceSurface
```

这次拆分只改变结构，不改变用户视觉、路由、Chat、Task、Workflow 或 Approval 行为。`DesktopSurfaceSkeleton` 不是 DesktopSpace，也不包含 Wallpaper、Widget、App Mount 或桌宠。

宿主选择由 `surfaceHost.ts` 解析；默认仍为 standalone。后续 v2 可以扩展 Desktop host，但不得复制一套 Workspace 页面或让 Workspace 继承 DesktopSpace。

若 Surface 拆分造成严重回归，应优先回滚独立提交 `refactor(surface): establish workspace surface host`，不得在坏基线上继续叠加领域功能。
