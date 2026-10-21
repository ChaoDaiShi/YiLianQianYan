use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::safety::{
    grant::{GrantEffect, GrantEvaluator, GrantResource, GrantSource, SecurityGrant},
    PermissionId, ResourceDescriptor,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    Full,
    Approve,
    Deny,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRule {
    pub id: String,
    pub scope: String,
    pub subject_id: Option<String>,
    pub permission_id: PermissionId,
    pub resource: Option<GrantResource>,
    pub mode: AccessMode,
}
impl AccessRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("规则 ID 不能为空".into());
        }
        match self.scope.as_str() {
            "default" if self.subject_id.is_none() && self.resource.is_none() => {}
            "agent" | "capability"
                if self
                    .subject_id
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty())
                    && self.resource.is_none() => {}
            "resource" if self.resource.is_some() => {}
            _ => return Err("规则作用范围或关联对象无效".into()),
        }
        if let Some(resource) = &self.resource {
            crate::safety::grant::validate_grant(self.permission_id, resource)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct AccessResolution {
    pub mode: AccessMode,
    pub matched_rule_ids: Vec<String>,
    pub effective_rule_id: String,
    pub reason: String,
}
impl Database {
    pub fn initialize_access_rules_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&ProductMigrationSpec::new(1020,"1020_scoped_access_rules",MigrationOwner::V1TaskWorld,
 "CREATE TABLE security_access_rule_scopes(grant_id TEXT PRIMARY KEY REFERENCES security_grants(grant_id) ON DELETE CASCADE,scope TEXT NOT NULL,subject_id TEXT);"))
    }
    pub fn list_access_rules(&self) -> Result<Vec<AccessRule>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT g.grant_id,s.scope,s.subject_id,g.permission_id,g.resource_json,g.effect FROM security_grants g JOIN security_access_rule_scopes s ON s.grant_id=g.grant_id WHERE g.subject_id='local-user' ORDER BY g.created_at,g.grant_id").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            let (id, scope, subject_id, permission, resource, effect) =
                row.map_err(|e| e.to_string())?;
            let resource: GrantResource =
                serde_json::from_str(&resource).map_err(|e| e.to_string())?;
            Ok(AccessRule {
                id,
                scope,
                subject_id,
                permission_id: serde_json::from_value(serde_json::Value::String(permission))
                    .map_err(|e| e.to_string())?,
                resource: if matches!(resource, GrantResource::Any) {
                    None
                } else {
                    Some(resource)
                },
                mode: match effect.as_str() {
                    "allow" => AccessMode::Full,
                    "approve" => AccessMode::Approve,
                    "deny" => AccessMode::Deny,
                    _ => return Err("无效的权限模式".into()),
                },
            })
        })
        .collect()
    }
    pub fn replace_access_rules(&self, rules: &[AccessRule]) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for rule in rules {
            rule.validate()?;
            if !seen.insert(&rule.id) {
                return Err("规则 ID 重复".into());
            }
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM security_grants WHERE grant_id IN (SELECT grant_id FROM security_access_rule_scopes)",[]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM security_access_rule_scopes", [])
            .map_err(|e| e.to_string())?;
        for rule in rules {
            let effect = match rule.mode {
                AccessMode::Full => "allow",
                AccessMode::Approve => "approve",
                AccessMode::Deny => "deny",
            };
            tx.execute("INSERT INTO security_grants(grant_id,subject_id,effect,permission_id,resource_json,source,created_at) VALUES(?1,'local-user',?2,?3,?4,'user',?5)",params![rule.id,effect,rule.permission_id.as_str(),serde_json::to_string(rule.resource.as_ref().unwrap_or(&GrantResource::Any)).map_err(|e|e.to_string())?,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
            tx.execute(
                "INSERT INTO security_access_rule_scopes VALUES(?1,?2,?3)",
                params![rule.id, rule.scope, rule.subject_id],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}
pub fn resolve_access_rule(
    rules: &[AccessRule],
    permission: PermissionId,
    resource: Option<&ResourceDescriptor>,
    agent: Option<&str>,
    tool: &str,
    workspace: &std::path::Path,
    registry: crate::isolation::SharedManagedProcessRegistry,
) -> Option<AccessResolution> {
    let mut matched = vec![];
    for rule in rules {
        if rule.permission_id != permission {
            continue;
        }
        let selected = match rule.scope.as_str() {
            "default" => true,
            "agent" => rule.subject_id.as_deref() == agent,
            "capability" => rule
                .subject_id
                .as_deref()
                .is_some_and(|id| id == tool || permission.as_str().split('.').next() == Some(id)),
            "resource" => rule
                .subject_id
                .as_deref()
                .is_none_or(|id| Some(id) == agent),
            _ => false,
        };
        if !selected {
            continue;
        }
        if let Some(bound) = &rule.resource {
            let Some(resource) = resource else { continue };
            let grant = SecurityGrant {
                id: rule.id.clone(),
                subject_id: "local-user".into(),
                effect: GrantEffect::Allow,
                permission,
                resource: bound.clone(),
                source: GrantSource::User,
                created_at: 0,
                expires_at: None,
            };
            let evaluation = GrantEvaluator::new(vec![grant], workspace)
                .with_registry(registry.clone())
                .evaluate_with_evidence(
                    permission,
                    resource,
                    chrono::Utc::now().timestamp_millis(),
                );
            if evaluation.matched_grant_id.is_none() {
                continue;
            }
        }
        matched.push(rule);
    }
    if matched.is_empty() {
        return None;
    }
    let precedence = |r: &AccessRule| match r.scope.as_str() {
        "resource" => 3,
        "capability" => 2,
        "agent" => 1,
        _ => 0,
    };
    let winner = matched
        .iter()
        .copied()
        .filter(|r| r.mode == AccessMode::Deny)
        .next()
        .or_else(|| {
            matched
                .iter()
                .copied()
                .max_by_key(|r| (precedence(r), usize::from(r.mode == AccessMode::Approve)))
        })?;
    Some(AccessResolution {
        mode: winner.mode,
        matched_rule_ids: matched.iter().map(|r| r.id.clone()).collect(),
        effective_rule_id: winner.id.clone(),
        reason: format!("{} 规则 {}", winner.scope, winner.id),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rule(id: &str, scope: &str, subject: Option<&str>, mode: AccessMode) -> AccessRule {
        AccessRule {
            id: id.into(),
            scope: scope.into(),
            subject_id: subject.map(str::to_owned),
            permission_id: PermissionId::FilesystemWrite,
            resource: None,
            mode,
        }
    }
    #[test]
    fn access_rules_priority_and_deny_precedence() {
        let mut rules = vec![
            rule("default", "default", None, AccessMode::Approve),
            rule("agent", "agent", Some("agent-a"), AccessMode::Full),
            rule(
                "tool",
                "capability",
                Some("write_file"),
                AccessMode::Approve,
            ),
        ];
        let resolve = |rules: &[AccessRule]| {
            resolve_access_rule(
                rules,
                PermissionId::FilesystemWrite,
                None,
                Some("agent-a"),
                "write_file",
                std::path::Path::new("."),
                std::sync::Arc::new(crate::isolation::ManagedProcessRegistry::new()),
            )
        };
        assert_eq!(resolve(&rules).unwrap().mode, AccessMode::Approve);
        rules.push(rule(
            "same-specificity",
            "capability",
            Some("write_file"),
            AccessMode::Full,
        ));
        assert_eq!(resolve(&rules).unwrap().mode, AccessMode::Approve);
        rules.push(rule("deny-default", "default", None, AccessMode::Deny));
        assert_eq!(resolve(&rules).unwrap().mode, AccessMode::Deny);
    }
    #[test]
    fn access_rules_persist_and_replace_atomically_without_touching_old_grants() {
        let path = std::env::temp_dir().join(format!("access-rule-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&path).unwrap();
        db.initialize_access_rules_schema().unwrap();
        db.create_grant(&SecurityGrant {
            id: "existing".into(),
            subject_id: "local-user".into(),
            effect: GrantEffect::Deny,
            permission: PermissionId::FilesystemWrite,
            resource: GrantResource::Filesystem {
                root: ".".into(),
                recursive: true,
            },
            source: GrantSource::User,
            created_at: 1,
            expires_at: None,
        })
        .unwrap();

        db.replace_access_rules(&[rule("full", "default", None, AccessMode::Full)])
            .unwrap();
        let mut invalid = rule("invalid", "resource", None, AccessMode::Deny);
        invalid.resource = None;
        assert!(db.replace_access_rules(&[invalid]).is_err());
        assert_eq!(db.list_access_rules().unwrap().len(), 1);
        assert_eq!(db.list_grants("local-user").unwrap().len(), 1);
        drop(db);
        let db = Database::new(&path).unwrap();
        assert_eq!(db.list_access_rules().unwrap()[0].mode, AccessMode::Full);
        db.replace_access_rules(&[]).unwrap();
        assert!(db.list_access_rules().unwrap().is_empty());
        assert_eq!(db.list_grants("local-user").unwrap()[0].id, "existing");
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
