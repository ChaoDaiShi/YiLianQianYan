use super::*;

/// Called inside the owning entity's deletion transaction.
pub(crate) fn remove_connector_entity_on(
    conn: &rusqlite::Connection,
    kind: &str,
    entity_id: &str,
) -> Result<(), rusqlite::Error> {
    let node = format!("{kind}:{entity_id}");
    conn.execute(
        "DELETE FROM connector_bindings WHERE source_id=?1 OR target_id=?1",
        [&node],
    )?;
    let layouts = {
        let mut stmt = conn.prepare("SELECT canvas_id,layout_json FROM connector_layouts")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (id, raw) in layouts {
        let mut layout: ConnectorLayout = serde_json::from_str(&raw)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let before = layout.nodes.len();
        layout.nodes.retain(|p| p.id != node);
        if before != layout.nodes.len() {
            conn.execute(
                "UPDATE connector_layouts SET layout_json=?2 WHERE canvas_id=?1",
                params![
                    id,
                    serde_json::to_string(&layout)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                ],
            )?;
        }
    }
    let displays = {
        let mut stmt = conn.prepare("SELECT id,display_json FROM connector_canvases")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (id, raw) in displays {
        let mut display: ConnectorDisplay = serde_json::from_str(&raw)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let before = display.hidden_node_ids.len();
        display.hidden_node_ids.retain(|p| p != &node);
        if before != display.hidden_node_ids.len() {
            conn.execute(
                "UPDATE connector_canvases SET display_json=?2 WHERE id=?1",
                params![
                    id,
                    serde_json::to_string(&display)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                ],
            )?;
        }
    }
    Ok(())
}

impl Database {
    pub fn connector_database_resources(&self) -> Result<Vec<ConnectorResource>, String> {
        let conn = self.conn();
        let mut all = vec![];
        for (kind, sql) in [
            ("agent", "SELECT id,name FROM agent_definitions"),
            ("conversation", "SELECT id,title FROM conversations"),
            (
                "workflow",
                "SELECT id,name FROM workflows UNION SELECT id,name FROM workflow_graphs",
            ),
            ("mcp", "SELECT id,name FROM mcp_servers"),
            ("model", "SELECT id,label FROM llm_models"),
            (
                "memory",
                "SELECT id,substr(content,1,80) FROM memories WHERE category!='knowledge'",
            ),
            (
                "knowledge",
                "SELECT id,substr(content,1,80) FROM memories WHERE category='knowledge'",
            ),
        ] {
            let mut stmt = conn.prepare(sql).map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(ConnectorResource::new(
                        kind,
                        &r.get::<_, String>(0)?,
                        &r.get::<_, String>(1)?,
                    ))
                })
                .map_err(err)?;
            all.extend(rows.collect::<Result<Vec<_>, _>>().map_err(err)?);
        }
        Ok(all)
    }
}
