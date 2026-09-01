# v1 / v2 Boundary

## v1 Task World

v1 拥有 Task、Workflow 使用体验、TaskGraph、执行状态与 Workspace 内交互。Workflow DAG 与未来 TaskGraph 必须分离：Workflow 是可执行定义，TaskGraph 是产品呈现/组织语义，二者可通过 adapter 关联但不得互相替代。

## v2 Desktop World

v2 拥有 DesktopSurface、DesktopSpace、Wallpaper、Widget、App discovery/mount、Desktop context 与 Windows 深层集成。DesktopSpace 必须是独立领域，不继承 Workspace；Workspace 只作为可被 Desktop Surface 承载的应用 Surface。

## 共享方式

v2 读取 v1 Task 只能消费 `TaskProjection`，不能理解 TaskGraph 内部。v1 获取桌面上下文只能消费 `DesktopContextProjection`，不能依赖 HWND 或 Window tree。跨域动作使用 Command，结果观察使用 Event。

Canvas layout 与执行语义分离。v1 可以研究 React Flow，但 v2 Desktop layout 不得被强制采用同一图库；最多共享布局 primitive 或 persistence contract。

Foundation 明确不规定 TaskGraph 数据库结构、DesktopSpace 继承模型、AppMount 的 HWND 技术或 Infinite Canvas 图库。Shared 到此冻结，后续领域实现不得“顺手”进入该层。
