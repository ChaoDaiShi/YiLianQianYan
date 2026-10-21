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

fn decode_grant_field<T: serde::de::DeserializeOwned>(
    raw: &str,
    column: usize,
) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

impl Database {
    pub fn list_grants(&self, subject_id: &str) -> Result<Vec<SecurityGrant>, rusqlite::Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT grant_id, subject_id, effect, permission_id, resource_json, source, created_at, expires_at
             FROM security_grants WHERE subject_id = ?1 AND grant_id NOT IN (SELECT grant_id FROM security_access_rule_scopes) ORDER BY created_at",
        )?;
        let rows = stmt.query_map(rusqlite::params![subject_id], |row| {
            let effect_raw: String = row.get(2)?;
            let permission_raw: String = row.get(3)?;
            let resource_raw: String = row.get(4)?;
            let source_raw: String = row.get(5)?;

            let effect = decode_grant_field::<super::model::GrantEffect>(&effect_raw, 2)?;
            let permission = decode_grant_field::<crate::safety::PermissionId>(&permission_raw, 3)?;
            let resource = serde_json::from_str::<super::model::GrantResource>(&resource_raw)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let source = decode_grant_field::<super::model::GrantSource>(&source_raw, 5)?;

            Ok(SecurityGrant {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                effect,
                permission,
                resource,
                source,
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
            let effect_raw: String = row.get(2)?;
            let permission_raw: String = row.get(3)?;
            let resource_raw: String = row.get(4)?;
            let source_raw: String = row.get(5)?;

            let effect = decode_grant_field::<super::model::GrantEffect>(&effect_raw, 2)?;
            let permission = decode_grant_field::<crate::safety::PermissionId>(&permission_raw, 3)?;
            let resource = serde_json::from_str::<super::model::GrantResource>(&resource_raw)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let source = decode_grant_field::<super::model::GrantSource>(&source_raw, 5)?;

            Ok(SecurityGrant {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                effect,
                permission,
                resource,
                source,
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
