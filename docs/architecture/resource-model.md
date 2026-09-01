# Resource Model

Resource 是共享输入身份，不是文件路径透传，也不是可执行授权。

元数据包含 `id`、`source`、规范化 `name`、`mime_type`、`size`、SHA-256 `hash`、应用管理的 `storage_path`、对象型 `metadata` 与时间戳。POST `/api/resources/ingest?name=...` 接收原始字节；GET `/api/resources` 和 `/api/resources/:id` 查询元数据。

安全规则：

- 默认最大 25 MiB，超限在写入前拒绝；
- 丢弃客户端原始绝对路径，只保留规范化文件名；
- 先以 create-new 临时文件写入并 sync，再原子 rename；
- 目标位于数据库同级的应用管理 `resources/`；
- 数据库落盘失败时删除对应文件；
- 成功后发出 `resource.created` 产品事件；
- 上传不代表允许执行或解析。

本 Foundation 不实现 OCR、PDF/Office 高级 Parser、向量化或自动执行。下游可通过 Resource ID 注册自己的 Parser/Indexer，而不改变身份契约。

Migration 1 创建 `resources` 表，属于 Shared 0–999 命名空间。该迁移是 forward-only；回滚应用代码不得删除用户资源或表，修复应使用新版本迁移。
