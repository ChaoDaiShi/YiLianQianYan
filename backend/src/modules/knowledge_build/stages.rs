use super::{ask, evidence, prompts::*, save, types::*};
use crate::{app::state::AppServer, integrations::llm::client::LlmClient};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StructureDraft {
    pub summary: String,
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
    pub limitations: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaimDraft {
    pub id: String,
    pub text: String,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceReference {
    pub source_id: String,
    pub quote_id: String,
    pub stance: EvidenceStance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaimsDraft {
    pub claims: Vec<ClaimDraft>,
    pub contradictions: Vec<Contradiction>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceAssessment {
    pub source_id: String,
    pub source_type: String,
    pub reason: String,
    pub basis_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssessmentsDraft {
    pub source_assessments: Vec<SourceAssessment>,
}

fn excerpts(sources: &[BuildSource]) -> Vec<Value> {
    sources
        .iter()
        .map(|source| {
            json!({
                "id":source.id,"title":source.title,"excerpt":source.excerpt,
                "evidence_kind":source.evidence_kind,"source_type":source.source_type
            })
        })
        .collect()
}

/// Stable, source-local references to contiguous original text. The model
/// selects a reference; only the server constructs the retained quotation.
fn quote_units(text: &str, prefix: &str) -> Vec<(String, String)> {
    let mut units = Vec::new();
    for sentence in text.split_inclusive(['\n', '。', '！', '？', '；']) {
        let characters: Vec<_> = sentence.chars().collect();
        for chunk in characters.chunks(300) {
            let quote: String = chunk.iter().collect();
            if !quote.trim().is_empty() {
                units.push((format!("{prefix}-{}", units.len() + 1), quote));
            }
        }
    }
    units
}

fn claim_materials(sources: &[BuildSource]) -> Vec<Value> {
    sources.iter().map(|source| json!({
        "id":source.id,"title":source.title,"evidence_kind":source.evidence_kind,
        "quotes":quote_units(&source.excerpt,"excerpt").into_iter().map(|(id,text)|json!({"id":id,"text":text})).collect::<Vec<_>>()
    })).collect()
}

fn assessment_materials(sources: &[BuildSource]) -> Vec<Value> {
    sources.iter().map(|source| json!({
        "id":source.id,
        "basis":quote_units(&source.title,"title").into_iter().chain(quote_units(&source.excerpt,"excerpt")).map(|(id,text)|json!({"id":id,"text":text})).collect::<Vec<_>>()
    })).collect()
}

fn resolve_claims(
    claims: Vec<ClaimDraft>,
    sources: &[BuildSource],
) -> Result<Vec<KnowledgeClaim>, String> {
    claims
        .into_iter()
        .map(|claim| {
            let evidence = claim
                .evidence
                .into_iter()
                .map(|reference| {
                    let source = sources
                        .iter()
                        .find(|source| source.id == reference.source_id)
                        .ok_or("论点引用了缺失来源")?;
                    let quote = quote_units(&source.excerpt, "excerpt")
                        .into_iter()
                        .find(|(id, _)| *id == reference.quote_id)
                        .ok_or("论点引用了缺失证据片段")?
                        .1;
                    Ok(ClaimEvidence {
                        source_id: reference.source_id,
                        quote,
                        stance: reference.stance,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(KnowledgeClaim {
                id: claim.id,
                text: claim.text,
                evidence,
                status: "unverified".into(),
            })
        })
        .collect()
}

pub(super) async fn synthesize_round(
    server: &AppServer,
    llm: &LlmClient,
    build: &mut KnowledgeBuild,
    round: usize,
    new_source_start: usize,
) -> Result<BuildResult, String> {
    let sources = excerpts(&build.sources);
    build.progress.stage = format!("第{round}轮：构建知识层级");
    save(server, build)?;
    let previous_structure = build.result.as_ref().map(|result| json!({
        "summary":result.summary,"nodes":result.nodes,"edges":result.edges,"limitations":result.limitations
    }));
    let structure: StructureDraft = ask(llm, STRUCTURE_PROMPT, json!({
        "stage":"structure","topic":build.topic,"sources":sources,"previous_structure":previous_structure
    }), &format!("第{round}轮知识结构"), 4096).await?;
    evidence::validate_structure(
        &structure.summary,
        &structure.nodes,
        &structure.edges,
        &build.sources,
    )
    .map_err(|error| format!("第{round}轮知识结构：{error}"))?;

    build.progress.stage = format!("第{round}轮：提取论点与核对证据");
    save(server, build)?;
    let previous_claims = build.result.as_ref().map(|result| {
        result
            .claims
            .iter()
            .map(|claim| {
                json!({
                    "id":claim.id,"text":claim.text
                })
            })
            .collect::<Vec<_>>()
    });
    let claims: ClaimsDraft = ask(llm, CLAIMS_PROMPT, json!({
        "stage":"claims","topic":build.topic,"sources":claim_materials(&build.sources),"structure":structure,"previous_claims":previous_claims
    }), &format!("第{round}轮论点核对"), 4096).await?;
    let mut result = BuildResult {
        summary: structure.summary,
        nodes: structure.nodes,
        edges: structure.edges,
        limitations: structure.limitations,
        claims: resolve_claims(claims.claims, &build.sources)
            .map_err(|error| format!("第{round}轮论点核对：{error}"))?,
        contradictions: claims.contradictions,
    };
    evidence::validate_result(&mut result, &build.sources)
        .map_err(|error| format!("第{round}轮论点核对：{error}"))?;

    // Previous rounds already assessed their immutable excerpts. Only newly
    // admitted URLs need a type assessment in this round.
    if new_source_start < build.sources.len() {
        build.progress.stage = format!("第{round}轮：评估新增来源");
        save(server, build)?;
        let assessments: AssessmentsDraft = ask(llm, ASSESSMENT_PROMPT, json!({
            "stage":"source_assessments","topic":build.topic,"sources":assessment_materials(&build.sources[new_source_start..])
        }), &format!("第{round}轮来源评估"), 4096).await?;
        apply_assessments(
            &mut build.sources[new_source_start..],
            assessments.source_assessments,
        )
        .map_err(|error| format!("第{round}轮来源评估：{error}"))?;
    }
    Ok(result)
}

fn apply_assessments(
    sources: &mut [BuildSource],
    assessments: Vec<SourceAssessment>,
) -> Result<(), String> {
    let mut assessed = HashSet::new();
    for assessment in assessments {
        if !assessed.insert(assessment.source_id.clone())
            || assessment.reason.trim().is_empty()
            || !matches!(
                assessment.source_type.as_str(),
                "official_documentation"
                    | "standard"
                    | "textbook"
                    | "course"
                    | "paper"
                    | "reference"
                    | "other"
                    | "unknown"
            )
        {
            return Err("模型返回无效来源类型判断".into());
        }
        let source = sources
            .iter_mut()
            .find(|source| source.id == assessment.source_id)
            .ok_or("来源类型判断引用缺失来源")?;
        let basis_quote = if assessment.source_type == "unknown" && assessment.basis_id.is_empty() {
            String::new()
        } else {
            quote_units(&source.title, "title")
                .into_iter()
                .chain(quote_units(&source.excerpt, "excerpt"))
                .find(|(id, _)| *id == assessment.basis_id)
                .ok_or("来源类型判断缺少可在实际标题或摘录定位的依据")?
                .1
        };
        if assessment.source_type != "unknown"
            && (basis_quote.trim().is_empty()
                || basis_quote.chars().count() > 300
                || (!source.title.contains(&basis_quote) && !source.excerpt.contains(&basis_quote)))
        {
            return Err("来源类型判断缺少可在实际标题或摘录定位的依据".into());
        }
        source.source_priority = match assessment.source_type.as_str() {
            "official_documentation" | "standard" | "textbook" | "course" | "paper" => "preferred",
            "reference" | "other" => "supporting",
            _ => "unclassified",
        }
        .into();
        source.source_type = assessment.source_type;
        source.source_type_reason = if basis_quote.is_empty() {
            assessment.reason
        } else {
            format!("{}；可定位依据：{}", assessment.reason, basis_quote)
        };
    }
    if assessed.len() != sources.len() {
        return Err("模型没有说明所有来源的材料类型".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> BuildSource {
        BuildSource {
            id: "s6".into(),
            title: "数据结构课程混合式教学研究".into(),
            url: "https://paper.example/6".into(),
            query: "数据结构 教学研究".into(),
            excerpt: "by 冯向阳 · 2021 — 构建“MOOC + SPOC + OJ. + 翻转课堂”的混合式教学体系。"
                .into(),
            evidence_kind: "snippet".into(),
            source_type: "unknown".into(),
            source_priority: "unclassified".into(),
            source_type_reason: "尚未分类".into(),
        }
    }

    #[test]
    fn selected_evidence_preserves_original_punctuation_in_claims_and_assessments() {
        let mut sources = vec![source()];
        let draft: ClaimsDraft = serde_json::from_value(json!({"claims":[{"id":"c1","text":"研究混合式教学","evidence":[{"source_id":"s6","quote_id":"excerpt-1","stance":"supports"}]}],"contradictions":[]})).unwrap();
        let claims = resolve_claims(draft.claims, &sources).unwrap();
        assert_eq!(claims[0].evidence[0].quote, sources[0].excerpt);
        assert!(claims[0].evidence[0].quote.contains("OJ."));
        let draft: AssessmentsDraft=serde_json::from_value(json!({"source_assessments":[{"source_id":"s6","source_type":"paper","reason":"摘录可见作者及年份","basis_id":"excerpt-1"}]})).unwrap();
        apply_assessments(&mut sources, draft.source_assessments).unwrap();
        assert!(sources[0].source_type_reason.ends_with(&sources[0].excerpt));
    }

    #[test]
    fn fabricated_evidence_references_and_missing_assessments_are_rejected() {
        let sources = vec![source()];
        for (source_id, quote_id) in [
            ("s6", "excerpt-999"),
            ("missing-source", "excerpt-1"),
            ("s6", "title-1"),
        ] {
            let claim:ClaimDraft=serde_json::from_value(json!({"id":"c1","text":"论点","evidence":[{"source_id":source_id,"quote_id":quote_id,"stance":"supports"}]})).unwrap();
            assert!(resolve_claims(vec![claim], &sources).is_err());
        }
        assert!(apply_assessments(&mut vec![source()], vec![]).is_err());
        let draft:AssessmentsDraft=serde_json::from_value(json!({"source_assessments":[{"source_id":"s6","source_type":"paper","reason":"作者及年份","basis_id":"excerpt-999"}]})).unwrap();
        assert!(apply_assessments(&mut vec![source()], draft.source_assessments).is_err());
    }

    #[test]
    fn retained_quote_units_are_contiguous_and_fit_the_unicode_character_limit() {
        let text = format!("{}。第二句保留原始标点 OJ.。", "汉".repeat(601));
        let units = quote_units(&text, "excerpt");
        assert_eq!(units.len(), 4);
        assert_eq!(
            units
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<String>(),
            text
        );
        for (index, (id, quote)) in units.iter().enumerate() {
            assert_eq!(id, &format!("excerpt-{}", index + 1));
            assert!(quote.chars().count() <= 300);
            assert!(text.contains(quote));
        }
    }
}
