# Canvas Stability 实施计划

按用户任务书自动执行，无子智能体。分支 `feat/v1-canvas-stability`，基线 `ce3dca2`，每阶段定向验证、提交、推送；禁止合并、tag、release。

- [x] C0：核对基线并建立独立工作区；记录行为契约与状态归属。
- [x] C1/C2：在 `canvas/model/reconcileCanvasNodes.test.ts` 覆盖六种合并、拖动竞态、分组隐藏、尺寸和对象复用；实现纯 reconciler 并接入 Canvas。运行 `npm test -- src/features/task-world`。
- [x] C3：在 Canvas 内提供自动布局、100%、适应全部与轨迹定位；首次恢复视口，移除隐式 fit；卡片保留固定区域。用视口纯函数测试及真实浏览器验证选择/刷新不改变镜头。
- [x] C4：增强 `canvasViewWriter.ts` 的旧版本重放、一次重试及失败重试入口；测试串行写、旧响应、连续冲突、读取失败和拖动待保存刷新。
- [x] C5：Execution Trail 按图顺序更新数据，选择和定位分离；验证运行时间变化不重新排序。
- [x] C6：扩展 `frontend/e2e/core-paths.mjs` 的真实后端场景（独立临时数据 + system Edge）；至少三个节点，拖动、延迟保存、真实语义/执行更新、平移缩放、重新进入、显式定位/布局及分组。失败先保存 stdout/stderr、浏览器日志/截图、端口归属、进程和退出码，再诊断。
- [x] C7：运行 `npm test`、`npm run build`、frontend architecture checks、real-backend E2E；审查差异，生成 `canvas-stability-report.md`，提交并确认远端 HEAD。

后端未修改时从架构工作区复制一次已验证二进制，记录源与目标 SHA256；不重新编译 Rust，不跑 Rust 全门禁。出现后端契约修改需求立即停止并报告 `CANVAS_BACKEND_CONTRACT_CHANGE_REQUIRED`。
