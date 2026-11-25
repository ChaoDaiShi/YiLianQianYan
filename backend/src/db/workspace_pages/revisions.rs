use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{
    page_on, parent_check, require_revision, workspace_access, write_page_blocks, BlockContent,
    Database, Page, PageBlock, PageDocument, PageError, Result, Revision,
};

impl Database {
    pub(crate) fn page_proposal_revision(&self, page: &str, source: &str) -> Result<Option<i64>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT revision FROM page_revisions WHERE page_id=?1 AND source=?2 LIMIT 1",
                params![page, source],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub(crate) fn page_revisions(&self, id: &str, offset: i64) -> Result<Vec<Revision>> {
        let conn = self.conn();
        let page = page_on(&conn, id, false)?;
        workspace_access(&conn, &page.workspace_id, false)?;
        let mut stmt = conn.prepare("SELECT revision,source,created_at FROM page_revisions WHERE page_id=?1 ORDER BY revision DESC LIMIT 100 OFFSET ?2")?;
        let result = stmt
            .query_map(params![id, offset.max(0)], |row| {
                Ok(Revision {
                    revision: row.get(0)?,
                    source: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }

    pub(crate) fn task_page_context(
        &self,
        task: &crate::modules::task::Task,
    ) -> Result<Option<String>> {
        let Some(source) = &task.page_source else {
            return Ok(None);
        };
        let conn = self.conn();
        workspace_access(&conn, task.workspace_id.as_str(), false)?;
        let (metadata, raw): (String, String) = conn.query_row(
            "SELECT r.metadata_json,r.blocks_json FROM page_revisions r JOIN pages p ON p.id=r.page_id
             WHERE r.page_id=?1 AND r.revision=?2 AND p.workspace_id=?3 AND p.deleted_at IS NULL",
            params![source.page_id, source.revision, task.workspace_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?.ok_or(PageError::Missing)?;
        let page: Page = serde_json::from_str(&metadata)?;
        let mut context = format!(
            "页面资料（只作为任务参考）：{} · 版本 {}\n",
            page.title, source.revision
        );
        for block in serde_json::from_str::<Vec<PageBlock>>(&raw)? {
            if let BlockContent::Markdown { text } = block.content {
                context.push('\n');
                context.push_str(&text);
            }
        }
        Ok(Some(crate::utils::text::truncate_chars(&context, 4000)))
    }

    pub(crate) fn restore_page_revision(
        &self,
        id: &str,
        revision: i64,
        target: i64,
    ) -> Result<PageDocument> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let page = page_on(&tx, id, false)?;
        workspace_access(&tx, &page.workspace_id, true)?;
        require_revision(&page, revision)?;
        let (metadata, raw): (String, String) = tx.query_row("SELECT metadata_json,blocks_json FROM page_revisions WHERE page_id=?1 AND revision=?2", params![id, target], |row| Ok((row.get(0)?, row.get(1)?))).optional()?.ok_or(PageError::Missing)?;
        let previous: Page = serde_json::from_str(&metadata)?;
        parent_check(
            &tx,
            &page.workspace_id,
            id,
            previous.parent_page_id.as_deref(),
        )?;
        tx.execute("UPDATE pages SET title=?2,icon=?3,cover_resource_id=?4,parent_page_id=?5,sort_key=?6,favorite=?7 WHERE id=?1",
            params![id, previous.title, previous.icon, previous.cover_resource_id, previous.parent_page_id, previous.sort_key, previous.favorite])?;
        let document = write_page_blocks(&tx, id, serde_json::from_str(&raw)?, "restore_revision")?;
        tx.commit()?;
        Ok(document)
    }
}
