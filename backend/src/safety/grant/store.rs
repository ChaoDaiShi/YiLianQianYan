// ============================================================
// Grant persistence — CRUD for the `security_grants` table.
// ============================================================

use crate::db::Database;

use super::model::SecurityGrant;

fn ser_to_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn string_to_ser<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::json!(s)).ok()
}

impl Database {
    pub fn list_grants(&self, subject_id: &str) -> Result<Vec<SecurityGrant>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at
             FROM security_grants WHERE subject_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(rusqlite::params![subject_id], |row| {
            Ok(SecurityGrant {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                effect: string_to_ser(&row.get::<_, String>(2)?)
                    .unwrap_or(super::model::GrantEffect::Allow),
                permission: string_to_ser(&row.get::<_, String>(3)?)
                    .unwrap_or(crate::safety::PermissionId::FilesystemRead),
                resource: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(4)?)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_else(|| super::model::GrantResource::Shell {
                        host_escape_acknowledged: false,
                    }),
                source: string_to_ser(&row.get::<_, String>(5)?)
                    .unwrap_or(super::model::GrantSource::User),
                created_at: row.get(6)?,
                expires_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_grant(&self, id: &str) -> Result<Option<SecurityGrant>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at
             FROM security_grants WHERE grant_id = ?1",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id], |row| {
            Ok(SecurityGrant {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                effect: string_to_ser(&row.get::<_, String>(2)?)
                    .unwrap_or(super::model::GrantEffect::Allow),
                permission: string_to_ser(&row.get::<_, String>(3)?)
                    .unwrap_or(crate::safety::PermissionId::FilesystemRead),
                resource: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(4)?)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_else(|| super::model::GrantResource::Shell {
                        host_escape_acknowledged: false,
                    }),
                source: string_to_ser(&row.get::<_, String>(5)?)
                    .unwrap_or(super::model::GrantSource::User),
                created_at: row.get(6)?,
                expires_at: row.get(7)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_grant(&self, grant: &SecurityGrant) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        let resource_json = serde_json::to_string(&grant.resource).unwrap_or_default();
        let effect = ser_to_string(&grant.effect);
        let permission = ser_to_string(&grant.permission);
        let source = ser_to_string(&grant.source);
        conn.execute(
            "INSERT INTO security_grants (grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                grant.id,
                grant.subject_id,
                effect,
                permission,
                resource_json,
                source,
                grant.created_at,
                grant.expires_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_grant(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM security_grants WHERE grant_id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub fn delete_grant_for_subject(
        &self,
        id: &str,
        subject_id: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.conn();
        let changed = conn.execute(
            "DELETE FROM security_grants WHERE grant_id = ?1 AND subject_id = ?2",
            rusqlite::params![id, subject_id],
        )?;
        Ok(changed == 1)
    }
}
