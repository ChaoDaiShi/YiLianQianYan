# Canvas 状态归属

沿用 Architecture R2 基线 `ce3dca2`，不新增后端契约、数据库字段或全局状态库。

| 状态 | 权威与职责 | 不得覆盖 |
|---|---|---|
| TaskGraph | 节点语义、依赖、执行引用；图顺序决定轨迹顺序 | 布局、镜头、临时选择 |
| ExecutionProjection | 状态、运行、失败、结果；通过现有图投影读取 | 位置、尺寸、分组、镜头 |
| CanvasDocument | 现有 CanvasView：持久化位置、尺寸、分组/折叠、保存视口；selection 仅为保存快照 | 已挂载画布的交互位置和镜头 |
| CanvasInteraction | ReactFlow 本地节点位置、测量、选择、拖动、临时镜头、未提交移动 | TaskGraph 语义或执行状态 |

首次加载 CanvasDocument 建立本地状态。之后通过纯 reconciler 合并节点数据，保持已有节点对象与交互字段；显式布局命令才应用布局。折叠节点仍留在本地模型中，以 hidden 控制显示，避免折叠/展开丢失未保存位置。

视口只在挂载时读取文档。ReactFlow 当前视口是当前交互权威；结束移动时提交保存，返回文档不重新设置镜头。选择与定位分开；定位和自动布局属于 Canvas feature，Page 只负责组合及提供投影、文档和保存命令。

现有 CanvasViewWriteQueue 串行化写入。旧版本冲突读取最新文档后重新应用原 mutation，只重试一次；保持其他最新字段，保留失败 mutation 供用户重试。错误不撤销本地交互。后端仍是最终持久化权威，不改变 REST/DB 结构。
