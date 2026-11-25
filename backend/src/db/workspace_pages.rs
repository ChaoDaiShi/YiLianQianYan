use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

const MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1123,
    "1123_workspace_pages",
    MigrationOwner::V1TaskWorld,
    include_str!("workspace_pages.sql"),
);
const TASK_PAGE_SOURCE: ProductMigrationSpec = ProductMigrationSpec::new(
    1124,
    "1124_task_page_source",
    MigrationOwner::V1TaskWorld,
    "ALTER TABLE tasks ADD COLUMN source_page_id TEXT REFERENCES pages(id);
     ALTER TABLE tasks ADD COLUMN source_page_revision INTEGER;",
);

mod model;
mod revisions;

pub(crate) use model::{
    BlockContent, BlockOperation, NewPage, PageBlock, PageDocument, PageError, PageList,
    PageMetadata, PageQuery, Revision,
};
use model::{Page, Result};

const SELECT: &str = "SELECT p.id,p.workspace_id,p.parent_page_id,p.title,p.icon,p.cover_resource_id,
    p.sort_key,p.favorite,p.last_visited_at,p.revision,p.created_at,p.updated_at,p.deleted_at,
    EXISTS(SELECT 1 FROM pages ch WHERE ch.parent_page_id=p.id AND ch.deleted_at IS NULL) FROM pages p";

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn row_page(row: &rusqlite::Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        parent_page_id: row.get(2)?,
        title: row.get(3)?,
        icon: row.get(4)?,
        cover_resource_id: row.get(5)?,
        sort_key: row.get(6)?,
        favorite: row.get(7)?,
        last_visited_at: row.get(8)?,
        revision: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        deleted_at: row.get(12)?,
        has_children: row.get(13)?,
    })
}

fn workspace_access(conn: &Connection, id: &str, write: bool) -> Result<()> {
    match conn
        .query_row("SELECT status FROM workspaces WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .as_deref()
    {
        Some("active") => Ok(()),
        Some("archived") if !write => Ok(()),
        Some("archived") => Err(PageError::Archived),
        _ => Err(PageError::Missing),
    }
}

fn page_on(conn: &Connection, id: &str, deleted: bool) -> Result<Page> {
    conn.query_row(
        &format!("{SELECT} WHERE p.id=?1 AND (p.deleted_at IS NULL OR ?2)"),
        params![id, deleted],
        row_page,
    )
    .optional()?
    .ok_or(PageError::Missing)
}

fn blocks_on(conn: &Connection, id: &str) -> Result<Vec<PageBlock>> {
    let mut stmt = conn.prepare(
        "SELECT block_id,content_json FROM page_blocks WHERE page_id=?1 ORDER BY sort_key",
    )?;
    let rows = stmt.query_map([id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (id, raw) = row?;
        Ok(PageBlock {
            id,
            content: serde_json::from_str(&raw)?,
        })
    })
    .collect()
}

fn require_revision(page: &Page, revision: i64) -> Result<()> {
    if page.revision != revision {
        return Err(PageError::Conflict("页面已有新版本，请刷新或比较后再保存"));
    }
    Ok(())
}

fn title(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(PageError::Invalid("页面标题应包含 1–200 个字符"));
    }
    Ok(value)
}

fn parent_check(conn: &Connection, workspace: &str, id: &str, parent: Option<&str>) -> Result<()> {
    let Some(parent) = parent else { return Ok(()) };
    let parent = page_on(conn, parent, false)?;
    if parent.workspace_id != workspace {
        return Err(PageError::Invalid("父页面应位于同一工作空间"));
    }
    let cyclic: bool = conn.query_row(
        "WITH RECURSIVE ancestor(id,parent_page_id) AS (
           SELECT id,parent_page_id FROM pages WHERE id=?1
           UNION SELECT p.id,p.parent_page_id FROM pages p JOIN ancestor a ON p.id=a.parent_page_id
         ) SELECT EXISTS(SELECT 1 FROM ancestor WHERE id=?2)",
        params![parent.id, id],
        |row| row.get(0),
    )?;
    if cyclic {
        return Err(PageError::Conflict("页面不能移入自身或自己的子页面"));
    }
    Ok(())
}

fn sibling_order(
    conn: &Connection,
    workspace: &str,
    parent: Option<&str>,
    id: &str,
    before: Option<&str>,
) -> Result<i64> {
    if before.is_none() {
        return Ok(conn.query_row(
            "SELECT COALESCE(MAX(sort_key),-1)+1 FROM pages WHERE workspace_id=?1 AND parent_page_id IS ?2 AND deleted_at IS NULL",
            params![workspace, parent], |row| row.get(0))?);
    }
    let mut stmt = conn.prepare("SELECT id FROM pages WHERE workspace_id=?1 AND parent_page_id IS ?2 AND deleted_at IS NULL AND id!=?3 ORDER BY sort_key,id")?;
    let mut ids = stmt
        .query_map(params![workspace, parent, id], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let at = ids
        .iter()
        .position(|sibling| Some(sibling.as_str()) == before)
        .ok_or(PageError::Invalid("排序位置不属于目标页面组"))?;
    ids.insert(at, id.to_owned());
    for (index, sibling) in ids.iter().enumerate() {
        if sibling != id {
            conn.execute(
                "UPDATE pages SET sort_key=?2 WHERE id=?1",
                params![sibling, index as i64],
            )?;
        }
    }
    Ok(at as i64)
}

fn record_revision(conn: &Connection, document: &PageDocument, source: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO page_revisions(page_id,revision,source,metadata_json,blocks_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![document.page.id, document.page.revision, source, serde_json::to_string(&document.page)?, serde_json::to_string(&document.blocks)?, now()],
    )?;
    Ok(())
}

fn save_blocks(conn: &Connection, page: &Page, blocks: &[PageBlock]) -> Result<()> {
    let ids = blocks
        .iter()
        .map(|block| block.id.as_str())
        .collect::<Vec<_>>();
    conn.execute(
        "DELETE FROM page_blocks WHERE page_id=?1 AND block_id NOT IN
         (SELECT value FROM json_each(?2))",
        params![page.id, serde_json::to_string(&ids)?],
    )?;
    for (index, block) in blocks.iter().enumerate() {
        if block.id.is_empty() || block.id.len() > 128 {
            return Err(PageError::Invalid("内容块标识无效"));
        }
        let (kind, link) = match &block.content {
            BlockContent::Markdown { text } => {
                if text.len() > 1_000_000 {
                    return Err(PageError::Invalid("单个 Markdown 块不能超过 1 MB"));
                }
                ("markdown", None)
            }
            BlockContent::PageLink { page_id } => {
                if page_on(conn, page_id, true)?.workspace_id != page.workspace_id {
                    return Err(PageError::Invalid("引用页面应位于同一工作空间"));
                }
                ("page_link", Some(("page", page_id.as_str())))
            }
            BlockContent::KnowledgeLink { document_id, .. } => (
                "knowledge_link",
                Some(("knowledge_document", document_id.as_str())),
            ),
            BlockContent::Resource { resource_id, .. } => {
                ("resource", Some(("resource", resource_id.as_str())))
            }
            BlockContent::Rich { message_id, .. } => {
                ("rich", Some(("message", message_id.as_str())))
            }
        };
        let raw = serde_json::to_string(&block.content)?;
        let changed = conn.execute(
            "INSERT INTO page_blocks(page_id,block_id,block_type,content_json,sort_key,revision,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?7) ON CONFLICT(page_id,block_id) DO UPDATE SET
             block_type=excluded.block_type, content_json=excluded.content_json, sort_key=excluded.sort_key,
             revision=excluded.revision, updated_at=excluded.updated_at
             WHERE page_blocks.content_json!=excluded.content_json OR page_blocks.sort_key!=excluded.sort_key",
            params![page.id, block.id, kind, raw, index as i64, page.revision, now()],
        )?;
        if changed == 0 {
            continue;
        }
        conn.execute(
            "DELETE FROM page_links WHERE page_id=?1 AND block_id=?2",
            params![page.id, block.id],
        )?;
        if let Some((target_kind, target_id)) = link {
            conn.execute(
                "INSERT INTO page_links(page_id,block_id,target_kind,target_id) VALUES(?1,?2,?3,?4)",
                params![page.id, block.id, target_kind, target_id],
            )?;
        }
    }
    Ok(())
}

fn position(blocks: &[PageBlock], id: &str) -> Result<usize> {
    blocks
        .iter()
        .position(|block| block.id == id)
        .ok_or(PageError::Invalid("内容块不存在"))
}

fn place(blocks: &mut Vec<PageBlock>, block: PageBlock, before: Option<&str>) -> Result<()> {
    let index = match before {
        Some(target) => position(blocks, target)?,
        None => blocks.len(),
    };
    blocks.insert(index, block);
    Ok(())
}

impl Database {
    pub(crate) fn initialize_workspace_pages(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&MIGRATION)?;
        self.apply_product_migration(&TASK_PAGE_SOURCE)
    }

    pub(crate) fn list_pages(&self, query: &PageQuery) -> Result<PageList> {
        let conn = self.conn();
        workspace_access(&conn, &query.workspace_id, false)?;
        let limit = query.limit.unwrap_or(40).clamp(1, 100);
        let offset = query.offset.max(0);
        let (filter, order) = match (query.view.as_deref(), query.q.as_deref().map(str::trim).filter(|q| !q.is_empty())) {
            (Some("trash"), _) => ("p.trash_root_id=p.id", "p.deleted_at DESC,p.id"),
            (_, Some(_)) => ("p.deleted_at IS NULL AND (instr(lower(p.title),lower(?4))>0 OR EXISTS(SELECT 1 FROM page_blocks b WHERE b.page_id=p.id AND instr(lower(json_extract(b.content_json,'$.text')),lower(?4))>0))", "p.updated_at DESC,p.id"),
            (Some("favorites"), None) => ("p.deleted_at IS NULL AND p.favorite=1", "p.updated_at DESC,p.id"),
            (Some("recent"), None) => ("p.deleted_at IS NULL AND p.last_visited_at IS NOT NULL", "p.last_visited_at DESC,p.id"),
            (Some("all"), None) => ("p.deleted_at IS NULL", "p.updated_at DESC,p.id"),
            _ => ("p.deleted_at IS NULL AND p.parent_page_id IS ?3", "p.sort_key,p.id"),
        };
        let sql = format!(
            "{SELECT} WHERE p.workspace_id=?1 AND {filter} ORDER BY {order} LIMIT ?2 OFFSET ?5"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut pages = stmt
            .query_map(
                params![
                    query.workspace_id,
                    limit + 1,
                    query.parent_page_id,
                    query.q.as_deref().map(str::trim),
                    offset
                ],
                row_page,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let next_offset = (pages.len() > limit as usize).then_some(offset + limit);
        pages.truncate(limit as usize);
        Ok(PageList { pages, next_offset })
    }

    pub(crate) fn workspace_page(&self, id: &str, visit: bool) -> Result<PageDocument> {
        let conn = self.conn();
        let page = page_on(&conn, id, false)?;
        workspace_access(&conn, &page.workspace_id, false)?;
        if visit {
            conn.execute("UPDATE pages SET last_visited_at=?2 WHERE id=?1 AND workspace_id IN(SELECT id FROM workspaces WHERE status='active')", params![id, now()])?;
        }
        let blocks = blocks_on(&conn, id)?;
        Ok(PageDocument { page, blocks })
    }

    pub(crate) fn create_page(&self, body: NewPage) -> Result<PageDocument> {
        let blocks = body
            .markdown
            .as_ref()
            .map(|text| {
                vec![PageBlock {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: BlockContent::Markdown { text: text.clone() },
                }]
            })
            .unwrap_or_default();
        self.create_page_with_blocks(body, blocks, "create")
    }

    pub(crate) fn create_page_with_blocks(
        &self,
        body: NewPage,
        blocks: Vec<PageBlock>,
        source: &str,
    ) -> Result<PageDocument> {
        let id = uuid::Uuid::new_v4().to_string();
        self.create_page_with_id(body, blocks, source, &id)
    }

    pub(crate) fn create_page_with_id(
        &self,
        body: NewPage,
        blocks: Vec<PageBlock>,
        source: &str,
        id: &str,
    ) -> Result<PageDocument> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        workspace_access(&tx, &body.workspace_id, true)?;
        match page_on(&tx, id, true) {
            Ok(page) => {
                if page.workspace_id != body.workspace_id || page.deleted_at.is_some() {
                    return Err(PageError::Conflict("提案已创建页面"));
                }
                return Ok(PageDocument {
                    page,
                    blocks: blocks_on(&tx, id)?,
                });
            }
            Err(PageError::Missing) => {}
            Err(error) => return Err(error),
        }
        parent_check(&tx, &body.workspace_id, id, body.parent_page_id.as_deref())?;
        let key = sibling_order(
            &tx,
            &body.workspace_id,
            body.parent_page_id.as_deref(),
            id,
            None,
        )?;
        tx.execute(
            "INSERT INTO pages(id,workspace_id,parent_page_id,title,sort_key,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,1,?6,?6)",
            params![id, body.workspace_id, body.parent_page_id, title(&body.title)?, key, now()],
        )?;
        let page = page_on(&tx, id, false)?;
        save_blocks(&tx, &page, &blocks)?;
        let document = PageDocument { page, blocks };
        record_revision(&tx, &document, source)?;
        tx.commit()?;
        Ok(document)
    }

    pub(crate) fn update_page_metadata(
        &self,
        id: &str,
        change: PageMetadata,
    ) -> Result<PageDocument> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = page_on(&tx, id, false)?;
        workspace_access(&tx, &previous.workspace_id, true)?;
        require_revision(&previous, change.revision)?;
        let parent = change
            .parent_page_id
            .clone()
            .unwrap_or(previous.parent_page_id.clone());
        let key = if parent != previous.parent_page_id || change.before_page_id.is_some() {
            parent_check(&tx, &previous.workspace_id, id, parent.as_deref())?;
            sibling_order(
                &tx,
                &previous.workspace_id,
                parent.as_deref(),
                id,
                change.before_page_id.as_deref(),
            )?
        } else {
            previous.sort_key
        };
        let name = title(change.title.as_deref().unwrap_or(&previous.title))?;
        let icon = change
            .icon
            .or(previous.icon)
            .filter(|value| !value.is_empty());
        let cover = change
            .cover_resource_id
            .or(previous.cover_resource_id)
            .filter(|value| !value.is_empty());
        if icon.as_ref().is_some_and(|icon| icon.chars().count() > 16) {
            return Err(PageError::Invalid("页面图标最多 16 个字符"));
        }
        if let Some(cover) = cover.as_deref() {
            let image: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM resources WHERE id=?1 AND mime_type LIKE 'image/%')",
                [cover],
                |row| row.get(0),
            )?;
            if !image {
                return Err(PageError::Invalid("封面图片资源不存在"));
            }
        }
        tx.execute(
            "UPDATE pages SET parent_page_id=?2,title=?3,icon=?4,cover_resource_id=?5,favorite=?6,sort_key=?7,revision=revision+1,updated_at=?8 WHERE id=?1",
            params![id, parent, name, icon, cover, change.favorite.unwrap_or(previous.favorite), key, now()],
        )?;
        let document = PageDocument {
            page: page_on(&tx, id, false)?,
            blocks: blocks_on(&tx, id)?,
        };
        record_revision(&tx, &document, "metadata")?;
        tx.commit()?;
        Ok(document)
    }

    pub(crate) fn patch_page_blocks(
        &self,
        id: &str,
        revision: i64,
        operations: Vec<BlockOperation>,
        source: &str,
    ) -> Result<PageDocument> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let page = page_on(&tx, id, false)?;
        workspace_access(&tx, &page.workspace_id, true)?;
        require_revision(&page, revision)?;
        let mut blocks = blocks_on(&tx, id)?;
        for operation in operations {
            match operation {
                BlockOperation::Insert { block, before } => {
                    if blocks.iter().any(|current| current.id == block.id) {
                        return Err(PageError::Invalid("内容块标识重复"));
                    }
                    place(&mut blocks, block, before.as_deref())?;
                }
                BlockOperation::Replace { block } => {
                    let index = position(&blocks, &block.id)?;
                    blocks[index] = block;
                }
                BlockOperation::Remove { id } => {
                    let index = position(&blocks, &id)?;
                    blocks.remove(index);
                }
                BlockOperation::Move { id, before } => {
                    let index = position(&blocks, &id)?;
                    let block = blocks.remove(index);
                    place(&mut blocks, block, before.as_deref())?;
                }
            }
        }
        let document = write_page_blocks(&tx, id, blocks, source)?;
        tx.commit()?;
        Ok(document)
    }

    pub(crate) fn set_page_trash(&self, id: &str, revision: i64, restore: bool) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let page = page_on(&tx, id, restore)?;
        workspace_access(&tx, &page.workspace_id, true)?;
        require_revision(&page, revision)?;
        let updated = if restore {
            if page.deleted_at.is_none() {
                return Err(PageError::Conflict("页面已恢复"));
            }
            parent_check(&tx, &page.workspace_id, id, page.parent_page_id.as_deref())?;
            let mut stmt = tx.prepare(
                "UPDATE pages SET deleted_at=NULL,trash_root_id=NULL,revision=revision+1,updated_at=?2
                 WHERE trash_root_id=?1 RETURNING id",
            )?;
            let rows = stmt
                .query_map(params![id, now()], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if rows.is_empty() {
                return Err(PageError::Conflict("请先恢复包含此页面的上级页面"));
            }
            rows
        } else {
            let mut stmt = tx.prepare(
                "WITH RECURSIVE subtree(id) AS (
                    SELECT id FROM pages WHERE id=?1 UNION ALL
                    SELECT p.id FROM pages p JOIN subtree s ON p.parent_page_id=s.id WHERE p.deleted_at IS NULL
                 ) UPDATE pages SET deleted_at=?2,trash_root_id=?1,revision=revision+1,updated_at=?2
                   WHERE id IN(SELECT id FROM subtree) RETURNING id",
            )?;
            let rows = stmt
                .query_map(params![id, now()], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for page_id in updated {
            let document = PageDocument {
                page: page_on(&tx, &page_id, true)?,
                blocks: blocks_on(&tx, &page_id)?,
            };
            record_revision(
                &tx,
                &document,
                if restore { "restore_trash" } else { "trash" },
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn write_page_blocks(
    conn: &Connection,
    id: &str,
    blocks: Vec<PageBlock>,
    source: &str,
) -> Result<PageDocument> {
    conn.execute(
        "UPDATE pages SET revision=revision+1,updated_at=?2 WHERE id=?1",
        params![id, now()],
    )?;
    let page = page_on(conn, id, false)?;
    save_blocks(conn, &page, &blocks)?;
    let document = PageDocument { page, blocks };
    record_revision(conn, &document, source)?;
    Ok(document)
}
