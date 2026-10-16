use std::collections::HashMap;

use crate::{
    db::SkillUsageHistory,
    modules::{
        memory_skill::{evidence::screen_evidence, repository::hash, service::MemorySkillService},
        task::{
            artifact::ArtifactSource,
            proofloop::{
                enhancement_contracts::SkillRecommendation,
                routing::{lexical_overlap_terms, lexical_similarity_bp},
            },
        },
    },
};

const MAX_RECOMMENDATIONS: usize = 3;
const MAX_CONTEXT_CHARS: usize = 4_000;

struct InstalledSkill {
    recommendation: SkillRecommendation,
    history: Vec<SkillUsageHistory>,
}

impl MemorySkillService {
    /// Load only the current reviewed installation and revalidate its saved
    /// evidence and managed-file hash before exposing it to the planner.
    pub fn planning_recommendations(
        &self,
        goal: &str,
        limit: usize,
    ) -> Result<Vec<SkillRecommendation>, String> {
        let limit = limit.min(MAX_RECOMMENDATIONS);
        if limit == 0 || goal.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.db.initialize_skill_candidates()?;
        self.db.initialize_skill_usage()?;

        let installed_rows = {
            let conn = self.db.conn();
            let mut query = conn
                .prepare(
                    "SELECT v.skill_name, v.version, v.content, v.content_hash,
                            c.source_json, c.lesson
                     FROM managed_skill_versions v
                     JOIN skill_candidates c ON c.id = v.candidate_id
                     WHERE v.active = 1 AND c.status = 'confirmed'
                       AND c.skill_name = v.skill_name AND c.skill_version = v.version
                     ORDER BY v.skill_name COLLATE NOCASE, v.version DESC",
                )
                .map_err(|_| "已审核 Skill 列表不可用")?;
            let rows = query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(|_| "已审核 Skill 列表不可用")?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| "已审核 Skill 记录无效")?
        };

        let mut installed = Vec::new();
        for (name, version, content, content_hash, sources_json, lesson) in installed_rows {
            if content_hash != hash(&content) {
                continue;
            }
            let path = self.store.root().join(&name).join("SKILL.md");
            if !self.store.is_editable(&path) {
                continue;
            }
            let Ok(on_disk) = std::fs::read_to_string(&path) else {
                continue;
            };
            if on_disk != content || hash(&on_disk) != content_hash {
                continue;
            }
            let Ok(sources) = serde_json::from_str::<Vec<ArtifactSource>>(&sources_json) else {
                continue;
            };
            {
                let conn = self.db.conn();
                if screen_evidence(&conn, &sources, &lesson).is_err() {
                    continue;
                }
            }
            let history = self.db.load_skill_usage_history(&name, version)?;
            installed.push(InstalledSkill {
                recommendation: SkillRecommendation {
                    skill_name: name,
                    version,
                    score_bp: 0,
                    evidence_report_ids: Vec::new(),
                    rationale: String::new(),
                    context_text: content,
                },
                history,
            });
        }

        let candidates: Vec<_> = installed
            .iter()
            .map(|skill| skill.recommendation.clone())
            .collect();
        let lexical = rank_skills(goal, &candidates);
        let mut by_key: HashMap<(String, u32), &InstalledSkill> = installed
            .iter()
            .map(|skill| {
                (
                    (
                        skill.recommendation.skill_name.clone(),
                        skill.recommendation.version,
                    ),
                    skill,
                )
            })
            .collect();
        let mut ranked = Vec::with_capacity(lexical.len());
        for mut recommendation in lexical {
            let Some(skill) =
                by_key.remove(&(recommendation.skill_name.clone(), recommendation.version))
            else {
                continue;
            };
            let (success_bp, report_ids) = history_summary(&skill.history);
            if let Some(success_bp) = success_bp {
                recommendation.score_bp =
                    ((u32::from(recommendation.score_bp) * 70 + u32::from(success_bp) * 30 + 50)
                        / 100) as u16;
            }
            recommendation.evidence_report_ids = report_ids;
            if !recommendation.evidence_report_ids.is_empty() {
                recommendation.rationale.push_str("；相关验收报告：");
                recommendation
                    .rationale
                    .push_str(&recommendation.evidence_report_ids.join("、"));
            }
            ranked.push(recommendation);
        }
        ranked.sort_by(|left, right| {
            right
                .score_bp
                .cmp(&left.score_bp)
                .then_with(|| left.skill_name.cmp(&right.skill_name))
                .then_with(|| right.version.cmp(&left.version))
        });
        Ok(trim_recommendation_context(ranked, limit))
    }
}

/// Rank an already filtered set of reviewed, installed skill versions.
/// The caller that reads persistent rows must filter inactive and unreviewed
/// versions before calling this pure function.
pub fn recommend_skills(
    goal: &str,
    installed: &[SkillRecommendation],
    limit: usize,
) -> Vec<SkillRecommendation> {
    trim_recommendation_context(rank_skills(goal, installed), limit.min(MAX_RECOMMENDATIONS))
}

fn rank_skills(goal: &str, installed: &[SkillRecommendation]) -> Vec<SkillRecommendation> {
    let mut ranked = installed
        .iter()
        .filter_map(|candidate| {
            let matched_terms = lexical_overlap_terms(goal, &candidate.context_text);
            let score_bp = lexical_similarity_bp(goal, &candidate.context_text);
            if matched_terms.is_empty() || score_bp == 0 {
                return None;
            }
            let mut recommendation = candidate.clone();
            recommendation.score_bp = score_bp;
            recommendation.rationale = format!("命中目标词：{}", matched_terms.join("、"));
            recommendation.evidence_report_ids.clear();
            Some(recommendation)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score_bp
            .cmp(&left.score_bp)
            .then_with(|| left.skill_name.cmp(&right.skill_name))
            .then_with(|| right.version.cmp(&left.version))
    });
    ranked
}

fn history_summary(history: &[SkillUsageHistory]) -> (Option<u16>, Vec<String>) {
    let mut per_report = HashMap::<String, (String, i64)>::new();
    for row in history {
        if let Some(report_id) = row.report_id.as_deref() {
            per_report
                .entry(report_id.to_owned())
                .or_insert((row.outcome.clone(), row.recorded_at_ms));
        }
    }
    let eligible = per_report
        .values()
        .filter(|(outcome, _)| matches!(outcome.as_str(), "pass" | "fail"))
        .count();
    let passes = per_report
        .values()
        .filter(|(outcome, _)| outcome == "pass")
        .count();
    let score = (eligible >= 5).then(|| ((passes * 10_000 + eligible / 2) / eligible) as u16);
    let mut reports: Vec<_> = per_report.into_iter().collect();
    reports.sort_by(|left, right| {
        right
            .1
             .1
            .cmp(&left.1 .1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let report_ids = reports
        .into_iter()
        .take(3)
        .map(|(report_id, _)| report_id)
        .collect();
    (score, report_ids)
}

fn trim_recommendation_context(
    mut recommendations: Vec<SkillRecommendation>,
    limit: usize,
) -> Vec<SkillRecommendation> {
    let mut remaining = MAX_CONTEXT_CHARS;
    let mut result = Vec::new();
    let count = recommendations.len().min(limit).min(MAX_RECOMMENDATIONS);
    for (index, mut recommendation) in recommendations.drain(..).take(count).enumerate() {
        if remaining == 0 {
            break;
        }
        let context: String = recommendation
            .context_text
            .chars()
            .take(remaining / (count - index))
            .collect();
        let used = context.chars().count();
        if used == 0 {
            continue;
        }
        recommendation.context_text = context;
        remaining -= used;
        result.push(recommendation);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::recommend_skills;
    use crate::modules::task::proofloop::enhancement_contracts::SkillRecommendation;

    fn candidate(name: &str, content: String) -> SkillRecommendation {
        SkillRecommendation {
            skill_name: name.into(),
            version: 1,
            score_bp: 0,
            evidence_report_ids: Vec::new(),
            rationale: String::new(),
            context_text: content,
        }
    }

    #[test]
    fn recommends_at_most_three_goal_relevant_skills_within_context_budget() {
        let installed = vec![
            candidate("downloads", "整理下载文件并归档".repeat(300)),
            candidate("documents", "下载整理文档".repeat(300)),
            candidate("records", "整理下载记录".repeat(300)),
            candidate("cooking", "煎炒炖煮家常菜".repeat(300)),
        ];

        let recommendations = recommend_skills("整理下载文件", &installed, 10);

        assert_eq!(recommendations.len(), 3);
        assert!(recommendations
            .iter()
            .all(|recommendation| recommendation.score_bp > 0));
        assert!(
            recommendations
                .iter()
                .map(|recommendation| recommendation.context_text.chars().count())
                .sum::<usize>()
                <= 4000
        );
        assert!(!recommendations
            .iter()
            .any(|recommendation| recommendation.skill_name == "cooking"));
    }

    #[test]
    fn recommendation_limit_can_reduce_the_default_maximum() {
        let installed = vec![
            candidate("one", "整理下载文件".into()),
            candidate("two", "下载文件整理".into()),
        ];

        assert_eq!(recommend_skills("整理下载文件", &installed, 1).len(), 1);
        assert!(recommend_skills("整理下载文件", &installed, 0).is_empty());
    }

    #[test]
    fn success_history_stays_unknown_until_five_distinct_reports_exist() {
        use super::history_summary;
        use crate::db::SkillUsageHistory;

        assert_eq!(history_summary(&[]).0, None);
        let failures = (0..5)
            .map(|index| SkillUsageHistory {
                goal_id: format!("goal-{index}"),
                goal_version: 1,
                draft_id: format!("draft-{index}"),
                report_id: Some(format!("report-{index}")),
                outcome: "fail".into(),
                recorded_at_ms: index,
            })
            .collect::<Vec<_>>();
        assert_eq!(history_summary(&failures).0, Some(0));
    }
}
