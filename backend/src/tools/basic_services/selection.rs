use super::{BasicServiceProfile, BasicServiceTool, ModelPool};

impl BasicServiceTool {
    pub(super) fn selected(&self) -> Result<(Option<ModelPool>, Vec<BasicServiceProfile>), String> {
        let explicit = if let Some(agent) = &self.agent_id {
            let binding = self
                .db
                .resolve_connector_bindings(agent)?
                .into_iter()
                .find(|b| {
                    b.kind == "agent_model" && b.service_kind.as_deref() == Some(self.kind.as_str())
                });
            if let Some(b) = binding {
                Some((
                    b.model_source.ok_or("模型来源缺失")?,
                    b.target_id
                        .strip_prefix("model:")
                        .ok_or("模型引用无效")?
                        .to_owned(),
                ))
            } else {
                None
            }
        } else {
            None
        };
        let (source, id) = if let Some(explicit) = explicit {
            explicit
        } else if let Some(pool) = self
            .db
            .get_model_pool_binding(&format!("service:{}", self.kind))?
        {
            ("pool".into(), pool)
        } else {
            (
                "profile".into(),
                self.db
                    .get_basic_service_default(&self.kind)?
                    .ok_or("请先设置该用途的默认来源")?,
            )
        };
        if source == "profile" {
            let profile = self
                .db
                .get_basic_service_profile(&id)?
                .ok_or("服务档案已删除")?;
            if profile.service_kind != self.kind {
                return Err("服务用途不匹配".into());
            }
            return Ok((None, vec![profile]));
        }
        if source != "pool" {
            return Err("服务来源无效".into());
        }
        let pool = self.db.get_model_pool(&id)?.ok_or("模型池已删除")?;
        if !pool.enabled || pool.service_kind != self.kind {
            return Err("模型池未启用或用途不匹配".into());
        }
        let mut profiles = Vec::new();
        for tier in &pool.tiers {
            for id in &tier.profile_ids {
                let p = self
                    .db
                    .get_basic_service_profile(id)?
                    .ok_or("池候选档案已删除")?;
                if p.service_kind != self.kind {
                    return Err("池候选用途不匹配".into());
                }
                profiles.push(p);
            }
        }
        Ok((Some(pool), profiles))
    }
}
