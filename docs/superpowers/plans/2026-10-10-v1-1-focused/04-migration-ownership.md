# P4：保留历史迁移，停止扩占 V2 编号

> 已完成。对应 R03；保留 12 条历史云迁移常量与白名单校验，将 V11Cloud 新范围调整为 3000..=3999，验证 V2 保持只读未改动。

## 事实与范围

CLIENT 的 `backend/src/db/migrations.rs` 把 V11Cloud 定义为 `2000—2999`。V2 同名文件把 V2DesktopWorld 定义为同一段。当前 CLIENT 的实际云注册只有 12 条：

```text
2000 2001 2002 2003 2004 2010 2011 2012 2013 2014 2015 2016
```

这是代码注册列表，**不表示所有用户数据库都已执行了这些迁移**。SERVER 的 PostgreSQL `0001—0005` 属于独立数据库和独立注册机制，不与客户端 SQLite 编号合并讨论。

修改文件仅为 CLIENT 的 `backend/src/db/migrations.rs`、`backend/src/db/cloud/schema.rs` 的注册/校验部分、对应迁移测试及一份编号说明。冻结 V2 只读，不修改。现有 `migrations/2010_*.sql` 至 `2016_*.sql` 及 2000—2004 内联 SQL 内容保持字节不变。

## 先盘点实际库，再提交规则

- [x] 在独立检查记录中列出本机开发库、应用正在使用的 `YILIAN_DATA_DIR` 或默认数据目录中的账号库；不扫描整块磁盘，不读取聊天内容。
- [x] 对每个库只读连接，查询 version/name/owner/digest。旧库没有表或没有 digest 字段时记录结构现状，不能为“盘点”运行迁移。
- [x] 将样本复制到隔离位置用于升级测试；在线 SQLite 使用现有 backup API，不能只复制 app.db 而遗漏 WAL。
- [x] 生产/其他用户库不可访问时写 `NOT_INSPECTED`；不推断未部署、不重编号。

只读检查的关键代码（脚本运行时从参数提供 DB 路径，输出仅迁移元数据）：

```python
from pathlib import Path
import sqlite3
import sys

db = Path(sys.argv[1]).resolve()
with sqlite3.connect(db.as_uri() + '?mode=ro', uri=True) as conn:
    exists = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations'"
    ).fetchone()
    if not exists:
        print('NO_MIGRATION_TABLE')
    else:
        columns = {row[1] for row in conn.execute('PRAGMA table_info(schema_migrations)')}
        digest = 'digest' if 'digest' in columns else 'NULL AS digest'
        for row in conn.execute(
            f'SELECT version,name,owner,{digest} FROM schema_migrations ORDER BY version'
        ):
            print(row)
```

不能调用 `Database::new` 进行盘点，因为构造本身会执行迁移。

## 冻结后的编号规则

| 区间 | 用途 | 本轮行为 |
|---|---|---|
| 0—999 | Shared/Core | 不变 |
| 1000—1999 | V1 task world | 不变 |
| 2000—2999 | V2 桌面模块原预留；CLIENT 历史云条目为固定例外 | CLIENT 不再注册任何新的 2xxx 云迁移 |
| 3000—3999 | 今后新增 V1.1 云客户端迁移 | 本轮没有实际新增 SQL 时不造空迁移 |

历史例外要求 version、name、owner、SQL digest **全匹配**。不能只允许 `2000..=2016`，否则会把不存在的 2005—2009 和被替换的 SQL 放行。

在 `cloud/schema.rs` 将现有 12 个常量组成 `pub(crate) const LEGACY_CLOUD_MIGRATIONS: &[ProductMigrationSpec]`；只整理注册引用，不改 SQL。`initialize_cloud_schema` 先注册这张固定表，未来的新 3xxx 表单独追加。

保留 MigrationOwner::V11Cloud 及落库字符串 `v1_1_cloud`。将其默认范围改为 3000—3999，在 `validate_spec` 中为固定历史集合增加准确匹配：

```rust
fn is_legacy_cloud_spec(spec: &MigrationSpec) -> bool {
    spec.owner == MigrationOwner::V11Cloud
        && crate::db::cloud::schema::LEGACY_CLOUD_MIGRATIONS.iter().any(|old| {
            old.version == spec.version
                && old.name == spec.name
                && old.owner == spec.owner
                && old.digest() == spec.digest()
        })
}

fn validate_spec(spec: &MigrationSpec) -> rusqlite::Result<()> {
    if spec.name.is_empty() {
        return Err(migration_error("migration name must not be empty"));
    }
    if is_legacy_cloud_spec(spec) {
        return Ok(());
    }
    validate_version(spec.owner, spec.version)
}
```

现有 `apply_migrations` 继续按 version 查询并核对数据库中的 name/owner/digest，已应用条目跳过；不改主键，不改旧 owner，不直接 UPDATE 历史编号。另保存基线 12 条 name/digest 测试快照，防止有人改 SQL 又同时改变“例外表”后只在空库测试通过。

**边界明确：这只消除继续占号，不解决 V1/V2 数据库直接合并。** 两条版本线目前分库，不能打开 V2 库当作 V1 升级。未来若真实提出 V1/V2 数据合并，应独立设计导入步骤；本轮不为它新增命名空间数据库、双注册表或历史重排。

## 验收

| 用例 | 输入 | 结果 |
|---|---|---|
| P4-01 | 已应用全部历史云迁移的样本副本 | 启动前后 12 条历史元数据与业务记录相同，无重复 SQL |
| P4-02 | 只应用早期 2000—2004 的样本 | 后续现有 2010—2016 正常补齐；再启动无变更 |
| P4-03 | 空库 | 完整建库成功，仍记录原历史编号 |
| P4-04 | 测试内构造 2017 或 2005 的云 spec | 拒绝；不存在假 3000 空迁移 |
| P4-05 | 测试内构造 3000 云 spec | 可应用；旧云 owner 字符串不变 |
| P4-06 | 改过历史 SQL、同编号不同 owner 的样本 | 明确失败且不执行其余产品 SQL |
| P4-07 | 冻结 V2 | HEAD、文件哈希、工作区状态保持不变 |

```powershell
cargo test -p yilian-backend db::migrations::tests
cargo test -p yilian-backend --test v11_profile_isolation
cargo fmt
cargo fmt --check
cargo check --workspace
```

完成后向其他工作包交付编号表、样本盘点的脱敏摘要和测试日志。不因无法盘点全部生产终端而重编号任何历史迁移。
