use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::learning::*;
use rusqlite::{params, OptionalExtension};

impl Database {
    pub fn initialize_learning(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&ProductMigrationSpec::new(1055,"1055_workspace_learning",MigrationOwner::V1TaskWorld,
        "CREATE TABLE learning_sessions(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),title TEXT NOT NULL,status TEXT NOT NULL,sources_json TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0,error TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
        CREATE INDEX learning_workspace ON learning_sessions(workspace_id,created_at);
        CREATE TABLE learning_notes(session_id TEXT NOT NULL REFERENCES learning_sessions(id) ON DELETE CASCADE,revision INTEGER NOT NULL,resource_id TEXT NOT NULL,parent_resource_id TEXT,created_at INTEGER NOT NULL,PRIMARY KEY(session_id,revision));
        CREATE TABLE learning_exercises(id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES learning_sessions(id) ON DELETE CASCADE,resource_id TEXT NOT NULL,correct_option_id TEXT NOT NULL,rubric TEXT NOT NULL);
        CREATE TABLE learning_answers(id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES learning_sessions(id) ON DELETE CASCADE,exercise_id TEXT NOT NULL REFERENCES learning_exercises(id),option_id TEXT NOT NULL,correct INTEGER NOT NULL,created_at INTEGER NOT NULL);"))?;
        self.conn().execute("UPDATE learning_sessions SET status='failed',error='应用重启中断生成，已保存的内容不会重新生成' WHERE status='generating'",[])?;
        Ok(())
    }
    pub fn create_learning_session(
        &self,
        workspace: &str,
        title: &str,
        sources: &[LearningSource],
    ) -> Result<LearningSession, String> {
        if title.trim().is_empty()
            || title.chars().count() > 300
            || sources.is_empty()
            || sources.len() > 4
        {
            return Err("请选择1-4份资料并填写学习主题".into());
        }
        let conn = self.conn();
        let active: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1 AND status='active')",
                [workspace],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !active {
            return Err("工作空间不存在或已归档".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO learning_sessions VALUES(?1,?2,?3,'generating',?4,0,NULL,?5,?5)",
            params![
                id,
                workspace,
                title,
                serde_json::to_string(sources).map_err(|e| e.to_string())?,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        self.learning_session(workspace, &id)
    }
    pub fn learning_session(&self, workspace: &str, id: &str) -> Result<LearningSession, String> {
        self.conn().query_row("SELECT id,workspace_id,title,status,sources_json,revision,error,created_at,updated_at FROM learning_sessions WHERE id=?1 AND workspace_id=?2",params![id,workspace],|r|{
            let sources:String=r.get(4)?;
            Ok(LearningSession{id:r.get(0)?,workspace_id:r.get(1)?,title:r.get(2)?,status:r.get(3)?,sources:serde_json::from_str(&sources).map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?,revision:r.get(5)?,error:r.get(6)?,created_at:r.get(7)?,updated_at:r.get(8)?})
        }).map_err(|e|e.to_string())
    }
    pub fn learning_sessions(&self, workspace: &str) -> Result<Vec<LearningSession>, String> {
        let ids = {
            let conn = self.conn();
            let mut stmt=conn.prepare("SELECT id FROM learning_sessions WHERE workspace_id=?1 ORDER BY created_at DESC").map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map([workspace], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        ids.iter()
            .map(|id| self.learning_session(workspace, id))
            .collect()
    }
    pub fn fail_learning_session(&self, id: &str, error: &str) -> Result<(), String> {
        self.conn().execute("UPDATE learning_sessions SET status='failed',error=?2,updated_at=?3 WHERE id=?1 AND status='generating'",params![id,error,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn cancel_learning_session(&self, workspace: &str, id: &str) -> Result<bool, String> {
        self.conn().execute("UPDATE learning_sessions SET status='cancelled',updated_at=?3 WHERE id=?1 AND workspace_id=?2 AND status='generating'",params![id,workspace,chrono::Utc::now().timestamp_millis()]).map(|changed|changed==1).map_err(|e|e.to_string())
    }
    pub fn complete_learning_session(
        &self,
        id: &str,
        note: &str,
        exercises: &[(String, GeneratedExercise)],
    ) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let sources: String = tx
            .query_row(
                "SELECT sources_json FROM learning_sessions WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        for source in
            serde_json::from_str::<Vec<LearningSource>>(&sources).map_err(|e| e.to_string())?
        {
            let matches: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM knowledge_documents WHERE id=?1 AND hash=?2)",
                    params![source.document_id, source.hash],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !matches {
                return Err("来源资料在生成期间发生变化，未采用本次生成".into());
            }
        }
        let changed=tx.execute("UPDATE learning_sessions SET status='ready',revision=1,updated_at=?2 WHERE id=?1 AND status='generating'",params![id,now]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("学习生成已不处于运行状态".into());
        }
        tx.execute(
            "INSERT INTO learning_notes VALUES(?1,1,?2,NULL,?3)",
            params![id, note, now],
        )
        .map_err(|e| e.to_string())?;
        for (resource, exercise) in exercises {
            tx.execute(
                "INSERT INTO learning_exercises VALUES(?1,?2,?3,?4,?5)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    id,
                    resource,
                    exercise.correct_option_id,
                    exercise.explanation
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn learning_notes(&self, session: &str) -> Result<Vec<LearningNote>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT revision,resource_id,parent_resource_id,created_at FROM learning_notes WHERE session_id=?1 ORDER BY revision DESC").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([session], |r| {
                Ok(LearningNote {
                    revision: r.get(0)?,
                    resource_id: r.get(1)?,
                    parent_resource_id: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn save_learning_note(
        &self,
        session: &str,
        expected: i64,
        resource: &str,
    ) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let changed=tx.execute("UPDATE learning_sessions SET revision=revision+1,updated_at=?3 WHERE id=?1 AND revision=?2 AND status='ready'",params![session,expected,now]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("learning_revision_conflict".into());
        }
        tx.execute("INSERT INTO learning_notes SELECT ?1,?2,?3,resource_id,?4 FROM learning_notes WHERE session_id=?1 AND revision=?5",params![session,expected+1,resource,now,expected]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn learning_exercise_resources(
        &self,
        session: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id,resource_id FROM learning_exercises WHERE session_id=?1 ORDER BY rowid",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([session], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn answer_learning_exercise(
        &self,
        session: &str,
        exercise: &str,
        request_id: &str,
        option: &str,
    ) -> Result<LearningAnswer, String> {
        if uuid::Uuid::parse_str(request_id).is_err() {
            return Err("invalid_answer_request_id".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let existing: Option<(String, String, String)> = tx
            .query_row(
                "SELECT session_id,exercise_id,option_id FROM learning_answers WHERE id=?1",
                [request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((old_session, old_exercise, old_option)) = existing {
            if old_session != session || old_exercise != exercise || old_option != option {
                return Err("answer_idempotency_conflict".into());
            }
        } else {
            let expected:String=tx.query_row("SELECT correct_option_id FROM learning_exercises WHERE id=?1 AND session_id=?2",params![exercise,session],|r|r.get(0)).map_err(|e|e.to_string())?;
            tx.execute(
                "INSERT INTO learning_answers VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    request_id,
                    session,
                    exercise,
                    option,
                    expected == option,
                    chrono::Utc::now().timestamp_millis()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.learning_answers(session)?
            .into_iter()
            .find(|answer| answer.id == request_id)
            .ok_or_else(|| "作答结果未找到".into())
    }
    pub fn learning_answers(&self, session: &str) -> Result<Vec<LearningAnswer>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT a.id,a.exercise_id,a.option_id,a.correct,e.correct_option_id,e.rubric,a.created_at FROM learning_answers a JOIN learning_exercises e ON e.id=a.exercise_id WHERE a.session_id=?1 ORDER BY a.created_at DESC,a.rowid DESC").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([session], |r| {
                let correct: bool = r.get(3)?;
                Ok(LearningAnswer {
                    id: r.get(0)?,
                    exercise_id: r.get(1)?,
                    option_id: r.get(2)?,
                    correct,
                    score: if correct { 1 } else { 0 },
                    grading_method: "deterministic_option_match".into(),
                    correct_option_id: r.get(4)?,
                    explanation: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> (Database, LearningSession, String) {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES('workspace','Workspace','active',1,1)",[]).unwrap();
        let (document, _) = db
            .import_knowledge(
                "source-resource",
                "source.txt",
                "text/plain",
                "source-hash",
                12,
                "事实依据",
            )
            .unwrap();
        let session = db
            .create_learning_session(
                "workspace",
                "Study",
                &[LearningSource {
                    document_id: document.id.clone(),
                    resource_id: document.resource_id,
                    name: document.name,
                    hash: document.hash,
                }],
            )
            .unwrap();
        let exercise = GeneratedExercise {
            question: "Which?".into(),
            options: vec![
                Choice {
                    id: "a".into(),
                    label: "A".into(),
                },
                Choice {
                    id: "b".into(),
                    label: "B".into(),
                },
            ],
            correct_option_id: "a".into(),
            explanation: "证据说明A".into(),
            sources: vec![Citation {
                document_id: document.id,
                quote: "事实依据".into(),
            }],
        };
        db.complete_learning_session(
            &session.id,
            "note-resource",
            &[("exercise-resource".into(), exercise)],
        )
        .unwrap();
        let id = db.learning_exercise_resources(&session.id).unwrap()[0]
            .0
            .clone();
        (db, session, id)
    }
    #[test]
    fn answers_are_deterministic_idempotent_and_scoped() {
        let (db, session, exercise) = ready();
        assert!(db.learning_answers(&session.id).unwrap().is_empty());
        assert!(db.learning_session("other-workspace", &session.id).is_err());
        let request = uuid::Uuid::new_v4().to_string();
        let answer = db
            .answer_learning_exercise(&session.id, &exercise, &request, "b")
            .unwrap();
        assert_eq!(answer.score, 0);
        assert_eq!(answer.correct_option_id, "a");
        db.answer_learning_exercise(&session.id, &exercise, &request, "b")
            .unwrap();
        assert_eq!(db.learning_answers(&session.id).unwrap().len(), 1);
        assert!(db
            .answer_learning_exercise(&session.id, &exercise, &request, "a")
            .is_err());
        let corrected = db
            .answer_learning_exercise(
                &session.id,
                &exercise,
                &uuid::Uuid::new_v4().to_string(),
                "a",
            )
            .unwrap();
        assert_eq!(corrected.score, 1);
        assert_eq!(db.learning_answers(&session.id).unwrap().len(), 2);
    }
    #[test]
    fn edited_notes_append_resource_versions_and_reject_stale_updates() {
        let (db, session, _) = ready();
        db.save_learning_note(&session.id, 1, "edited-resource")
            .unwrap();
        assert!(db
            .save_learning_note(&session.id, 1, "stale-resource")
            .is_err());
        let versions = db.learning_notes(&session.id).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(
            versions[0].parent_resource_id.as_deref(),
            Some("note-resource")
        );
        assert_eq!(versions[1].resource_id, "note-resource");
    }
}
