# Migration Policy

## Version ownership

| 范围 | Owner |
| --- | --- |
| 0–999 | Shared/Core |
| 1000–1999 | v1 Task World |
| 2000–2999 | v2 Desktop World |
| 3000+ | 必须先在 integration 分配 |

版本 0 表示 v0.9 bootstrap/adoption；版本 1 是 `resources`。所有新增迁移必须有唯一 version/name/owner 并在事务中同时写 schema 与 `schema_migrations` 记录。

## Compatibility

启动仍先执行既有 v0.9 `CREATE TABLE IF NOT EXISTS` bootstrap，再运行 versioned migrations。已有 v0.9 数据库被记录为 `0000_v09_adopted`，新数据库为 `0000_v09_bootstrap`；二者都不得丢失既有用户数据。重复启动必须幂等。

禁止重写所有历史 schema、按猜测合并数据库、drop/rename 用户表或在回滚中删除数据。SQLite 继续作为本地持久层。

## Rollback

Migration 默认 forward-only。若不可逆：先备份、文档说明、使用后续版本 forward-fix，并尽量让旧程序仍可识别原有范围。回滚代码提交不自动回滚数据库；不得通过 destructive SQL 恢复版本号。

测试至少覆盖 fresh DB、包含真实既有行的 v0.9 adoption，以及重复启动。v1/v2 分支对越界版本必须 fail closed。
