use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::BTreeMap;

// Recover only uploads whose recorded resource reference and upload interval identify the same user turn.
const HISTORICAL_ATTACHMENT_SQL: &str = r#"WITH recorded_calls AS (
    SELECT m.conversation_id,m.created_at AS call_at,m.rowid AS call_rowid,
           json_extract(CASE WHEN json_valid(json_extract(call.value,'$.function.arguments')) THEN json_extract(call.value,'$.function.arguments') ELSE '{}' END,'$.resource_id') AS resource_id,
           (SELECT u.id FROM messages u WHERE u.conversation_id=m.conversation_id AND u.role='user' AND (u.created_at<m.created_at OR (u.created_at=m.created_at AND u.rowid<m.rowid)) ORDER BY u.created_at DESC,u.rowid DESC LIMIT 1) AS user_message_id
    FROM messages m JOIN json_each(CASE WHEN json_valid(m.tool_calls) THEN m.tool_calls ELSE '[]' END) call
), located_uploads AS (
    SELECT DISTINCT c.user_message_id,c.resource_id,b.created_at,b.id AS binding_id
    FROM recorded_calls c JOIN messages u ON u.id=c.user_message_id
    JOIN resource_bindings b ON b.resource_id=c.resource_id AND b.target_kind='conversation' AND b.target_id=c.conversation_id AND b.owner='local-user'
    WHERE typeof(c.resource_id)='text' AND b.created_at<=u.created_at
      AND NOT EXISTS (SELECT 1 FROM recorded_calls earlier WHERE earlier.conversation_id=c.conversation_id AND earlier.resource_id=c.resource_id AND (earlier.call_at<c.call_at OR (earlier.call_at=c.call_at AND earlier.call_rowid<c.call_rowid)))
      AND NOT EXISTS (SELECT 1 FROM messages p WHERE p.conversation_id=u.conversation_id AND p.role='user' AND (p.created_at<u.created_at OR (p.created_at=u.created_at AND p.rowid<u.rowid)) AND p.created_at>=b.created_at)
      AND NOT EXISTS (SELECT 1 FROM message_attachments a JOIN messages owner ON owner.id=a.message_id WHERE a.resource_id=c.resource_id AND owner.conversation_id=c.conversation_id)
)
INSERT OR IGNORE INTO message_attachments(message_id,resource_id,position)
SELECT user_message_id,resource_id,row_number() OVER (PARTITION BY user_message_id ORDER BY created_at,binding_id)-1 FROM located_uploads;"#;

impl Database {
    pub fn initialize_conversation_presentation(&self) -> Result<(), rusqlite::Error> {
        self.initialize_resource_bindings()
            .map_err(rusqlite::Error::InvalidParameterName)?;
        self.apply_product_migration(&ProductMigrationSpec::new(1061, "1061_message_attachments_and_origin", MigrationOwner::V1TaskWorld,
            "CREATE TABLE message_attachments(message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,resource_id TEXT NOT NULL REFERENCES resources(id),position INTEGER NOT NULL,PRIMARY KEY(message_id,resource_id));
             INSERT INTO settings(key,value) SELECT 'conversation_origin:'||s.conversation_id,json_object('kind','channel','provider',json_extract(c.payload,'$.provider'),'channel_name',json_extract(c.payload,'$.name')) FROM channel_sessions s JOIN external_channels c ON c.id=s.channel_id WHERE 1 ON CONFLICT(key) DO NOTHING;
             CREATE TRIGGER channel_session_origin AFTER INSERT ON channel_sessions BEGIN
               INSERT OR REPLACE INTO settings(key,value) SELECT 'conversation_origin:'||NEW.conversation_id,json_object('kind','channel','provider',json_extract(payload,'$.provider'),'channel_name',json_extract(payload,'$.name')) FROM external_channels WHERE id=NEW.channel_id;
             END;"))?;
        self.apply_product_migration(&ProductMigrationSpec::new(
            1062,
            "1062_recorded_attachment_locations",
            MigrationOwner::V1TaskWorld,
            HISTORICAL_ATTACHMENT_SQL,
        ))
    }

    pub fn message_attachments(
        &self,
        conversation: &str,
    ) -> Result<BTreeMap<String, Vec<Value>>, String> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT a.message_id,r.id,r.name,r.mime_type,r.size FROM message_attachments a JOIN messages m ON m.id=a.message_id JOIN resources r ON r.id=a.resource_id WHERE m.conversation_id=?1 ORDER BY m.created_at,a.position").map_err(|e|e.to_string())?;
        let rows = statement.query_map([conversation], |r| Ok((r.get::<_,String>(0)?, json!({"id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"mime_type":r.get::<_,String>(3)?,"size":r.get::<_,i64>(4)?})))).map_err(|e|e.to_string())?;
        let mut result = BTreeMap::<String, Vec<Value>>::new();
        for row in rows {
            let (id, attachment) = row.map_err(|e| e.to_string())?;
            result.entry(id).or_default().push(attachment);
        }
        Ok(result)
    }

    pub fn conversation_origins(&self) -> Result<BTreeMap<String, Value>, String> {
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT c.id,s.value FROM conversations c LEFT JOIN settings s ON s.key='conversation_origin:'||c.id").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut result = BTreeMap::new();
        for row in rows {
            let (id, raw) = row.map_err(|e| e.to_string())?;
            result.insert(
                id,
                match raw {
                    Some(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string())?,
                    None => json!({"kind":"desktop"}),
                },
            );
        }
        let mut statement=conn.prepare("SELECT b.conversation_id,m.conversation_id FROM message_canvas_branches b JOIN messages m ON m.id=b.parent_node_id UNION SELECT b.conversation_id,m.conversation_id FROM connector_message_runs b JOIN messages m ON m.id=b.parent_node_id").map_err(|e|e.to_string())?;
        let parents = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for _ in 0..parents.len() {
            for (child, parent) in &parents {
                if let Some(origin) = result
                    .get(parent)
                    .filter(|origin| origin["kind"] == "channel")
                    .cloned()
                {
                    result.insert(child.clone(), origin);
                }
            }
        }
        Ok(result)
    }

    pub fn present_conversation(&self, mut value: Value) -> Result<Value, String> {
        let id = value["id"].as_str().ok_or("会话缺少标识")?.to_owned();
        let attachments = self.message_attachments(&id)?;
        value["origin"] = self
            .conversation_origins()?
            .remove(&id)
            .ok_or("会话不存在")?;
        if let Some(messages) = value["messages"].as_array_mut() {
            for message in messages {
                if let Some(items) = message["id"].as_str().and_then(|id| attachments.get(id)) {
                    message["attachments"] = json!(items);
                }
            }
        }
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT r.id,r.name,r.mime_type,r.size FROM resource_bindings b JOIN resources r ON r.id=b.resource_id WHERE b.target_kind='conversation' AND b.target_id=?1 AND b.owner='local-user' AND NOT EXISTS(SELECT 1 FROM message_attachments a JOIN messages m ON m.id=a.message_id WHERE a.resource_id=r.id AND m.conversation_id=?1) ORDER BY b.created_at").map_err(|e|e.to_string())?;
        let items=statement.query_map([id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"mime_type":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        value["unassigned_attachments"] = json!(items);
        Ok(value)
    }
}

pub fn copy_message_attachments(
    conn: &rusqlite::Connection,
    from: &str,
    to: &str,
) -> Result<(), String> {
    conn.execute("INSERT INTO message_attachments(message_id,resource_id,position) SELECT ?2,resource_id,position FROM message_attachments WHERE message_id=?1",params![from,to]).map_err(|e|e.to_string())?;
    conn.execute("INSERT OR IGNORE INTO resource_bindings(id,resource_id,target_kind,target_id,node_id,owner,created_at) SELECT lower(hex(randomblob(16))),a.resource_id,'conversation',m.conversation_id,'','local-user',m.created_at FROM message_attachments a JOIN messages m ON m.id=a.message_id WHERE a.message_id=?1",[to]).map_err(|e|e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::{Database, MessageRow, ResourceTarget};
    use rusqlite::params;

    #[test]
    fn historical_attachment_uses_recorded_resource_reference_and_upload_turn() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = db.create_conversation("旧附件的位置").unwrap().id;
        for resource in ["image", "unreferenced", "after-turn"] {
            db.conn().execute("INSERT INTO resources(id,source,name,mime_type,size,hash,storage_path,metadata_json,created_at,updated_at) VALUES(?1,'upload',?1,'image/png',12,?1,?1,'{}',1,1)",[resource]).unwrap();
            db.bind_resources(
                &ResourceTarget::Conversation {
                    conversation_id: id.clone(),
                },
                &[resource.into()],
            )
            .unwrap();
            db.conn()
                .execute(
                    "UPDATE resource_bindings SET created_at=?2 WHERE resource_id=?1",
                    params![resource, if resource == "after-turn" { 21 } else { 17 }],
                )
                .unwrap();
        }
        for (mid, created) in [
            ("hello", 1),
            ("upload-question", 20),
            ("later-question", 50),
        ] {
            db.conn().execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES(?1,?2,'user',?1,?3)",params![mid,id,created]).unwrap();
        }
        let calls=serde_json::json!([{"function":{"name":"vision_analysis","arguments":"{\"resource_id\":\"image\"}"}},{"function":{"name":"vision_analysis","arguments":"{\"resource_id\":\"after-turn\"}"}}]).to_string();
        for (mid, created) in [("original-vision", 22), ("later-reuse", 52)] {
            db.conn().execute("INSERT INTO messages(id,conversation_id,role,content,tool_calls,created_at) VALUES(?1,?2,'assistant','',?3,?4)",params![mid,id,calls,created]).unwrap();
        }
        db.conn()
            .execute_batch(super::HISTORICAL_ATTACHMENT_SQL)
            .unwrap();
        let map = db.message_attachments(&id).unwrap();
        assert_eq!(map.get("upload-question").map(Vec::len), Some(1));
        assert_eq!(map["upload-question"][0]["id"], "image");
        assert!(!map.contains_key("hello"));
        assert!(!map.contains_key("later-question"));
        db.conn()
            .execute_batch(super::HISTORICAL_ATTACHMENT_SQL)
            .unwrap();
        assert_eq!(
            db.message_attachments(&id).unwrap()["upload-question"].len(),
            1
        );
    }

    fn input(conversation: &str, id: &str) -> MessageRow {
        MessageRow {
            id: id.into(),
            conversation_id: conversation.into(),
            role: "user".into(),
            content: "查看附件".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: 100,
        }
    }

    #[test]
    fn attachments_belong_to_the_exact_message_after_bindings_change() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = db.create_conversation("桌面附件").unwrap().id;
        db.conn().execute("INSERT INTO resources(id,source,name,mime_type,size,hash,storage_path,metadata_json,created_at,updated_at) VALUES('image','upload','截图.png','image/png',12,'hash','managed/image','{}',1,1)", []).unwrap();
        let bindings = db
            .bind_resources(
                &ResourceTarget::Conversation {
                    conversation_id: id.clone(),
                },
                &["image".into()],
            )
            .unwrap();
        db.accept_conversation_turn_with_resources(&input(&id, "u1"), &["image".into()])
            .unwrap();
        db.accept_conversation_turn(&input(&id, "u2")).unwrap();
        db.unbind_resource(&bindings[0].id).unwrap();
        let attachments = db.message_attachments(&id).unwrap();
        assert_eq!(attachments["u1"][0]["name"], "截图.png");
        assert!(!attachments.contains_key("u2"));
        db.conn()
            .execute("DELETE FROM messages WHERE id='u1'", [])
            .unwrap();
        assert!(db.message_attachments(&id).unwrap().is_empty());
    }

    #[test]
    fn fork_copies_attachment_identity_and_inherits_real_channel_origin() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = db.create_conversation("渠道对话").unwrap().id;
        let desktop = db.create_conversation("普通桌面对话").unwrap().id;
        db.conn().execute("INSERT INTO resources(id,source,name,mime_type,size,hash,storage_path,metadata_json,created_at,updated_at) VALUES('doc','upload','资料.docx','application/vnd.openxmlformats-officedocument.wordprocessingml.document',12,'hash','managed/doc','{}',1,1)", []).unwrap();
        db.bind_resources(
            &ResourceTarget::Conversation {
                conversation_id: id.clone(),
            },
            &["doc".into()],
        )
        .unwrap();
        db.accept_conversation_turn_with_resources(&input(&id, "user-doc"), &["doc".into()])
            .unwrap();
        db.conn().execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('answer',?1,'assistant','源回答',101)", [&id]).unwrap();
        db.conn()
            .execute(
                "INSERT INTO external_channels(id,payload) VALUES('qq',?1)",
                [r#"{"provider":"onebot11","name":"QQ 助手"}"#],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO channel_sessions(id,channel_id,peer_id,conversation_id,created_at) VALUES('session','qq','peer',?1,1)",
                [&id],
            )
            .unwrap();
        let branch = db.fork_message_canvas_conversation(&id, "answer").unwrap();
        let map = db.message_attachments(&branch).unwrap();
        assert_eq!(map.values().next().unwrap()[0]["id"], "doc");
        assert_eq!(
            db.resource_bindings(&ResourceTarget::Conversation {
                conversation_id: branch.clone()
            })
            .unwrap()
            .len(),
            1
        );
        let origins = db.conversation_origins().unwrap();
        assert_eq!(origins[&branch]["provider"], "onebot11");
        assert_eq!(origins[&desktop]["kind"], "desktop");
    }

    #[test]
    fn unrelated_resources_do_not_leave_a_partially_accepted_message() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = db.create_conversation("附件校验").unwrap().id;
        assert!(db
            .accept_conversation_turn_with_resources(&input(&id, "rejected"), &["missing".into()])
            .is_err());
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM messages WHERE id=?1",
                params!["rejected"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
