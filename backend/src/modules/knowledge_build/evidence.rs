use super::types::*;
use std::collections::{HashMap, HashSet};

/// Validate references against the actual retained search evidence. This proves
/// quotation provenance, not the truth of a claim or editorial independence.
pub fn validate_result(result: &mut BuildResult, sources: &[BuildSource]) -> Result<(), String> {
    if result.summary.trim().is_empty()
        || result.nodes.len() < 2
        || result.nodes.len() > 80
        || result.claims.is_empty()
        || result.claims.len() > 80
    {
        return Err("模型未返回完整的有来源知识结构（2–80节点、1–80论点）".into());
    }
    let source_map: HashMap<_, _> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut node_map = HashMap::new();
    for node in &result.nodes {
        if node.id.trim().is_empty()
            || node.title.trim().is_empty()
            || node.summary.trim().is_empty()
            || node_map.insert(node.id.as_str(), node).is_some()
        {
            return Err("知识节点缺少标识、内容或标识重复".into());
        }
        if node.source_ids.is_empty()
            || node
                .source_ids
                .iter()
                .any(|id| !source_map.contains_key(id.as_str()))
        {
            return Err("知识节点引用了缺失来源".into());
        }
    }
    if !result.nodes.iter().any(|node| node.parent_id.is_some()) {
        return Err("知识结构没有层级关系".into());
    }
    for node in &result.nodes {
        let mut seen = HashSet::from([node.id.as_str()]);
        let mut parent = node.parent_id.as_deref();
        while let Some(id) = parent {
            if !seen.insert(id) {
                return Err("知识层级包含循环".into());
            }
            parent = node_map
                .get(id)
                .ok_or("知识层级引用了缺失节点")?
                .parent_id
                .as_deref();
        }
    }
    for edge in &result.edges {
        if edge.from == edge.to
            || !node_map.contains_key(edge.from.as_str())
            || !node_map.contains_key(edge.to.as_str())
        {
            return Err("知识图谱关系引用无效节点".into());
        }
    }
    let mut claim_ids = HashSet::new();
    for claim in &mut result.claims {
        if claim.id.trim().is_empty()
            || claim.text.trim().is_empty()
            || !claim_ids.insert(claim.id.clone())
        {
            return Err("论点缺少标识、内容或标识重复".into());
        }
        let mut supporting_hosts = HashSet::new();
        let mut has_contradiction = false;
        for evidence in &claim.evidence {
            let source = source_map
                .get(evidence.source_id.as_str())
                .ok_or("论点引用了缺失来源")?;
            if evidence.quote.trim().is_empty()
                || evidence.quote.chars().count() > 500
                || !source.excerpt.contains(&evidence.quote)
            {
                return Err("论点引文不能在实际检索摘录中逐字定位".into());
            }
            if source.evidence_kind == "title_only" {
                return Err("只有标题的搜索结果不能作为论点证据".into());
            }
            match evidence.stance {
                EvidenceStance::Supports => {
                    let url = url::Url::parse(&source.url).map_err(|_| "来源URL无效")?;
                    let host = url
                        .host_str()
                        .ok_or("来源URL缺少主机")?
                        .trim_start_matches("www.")
                        .to_ascii_lowercase();
                    supporting_hosts.insert(host);
                }
                EvidenceStance::Contradicts => has_contradiction = true,
            }
        }
        // Distinct hosts are necessary but not proof of independent reporting.
        // Keep the scope explicit in limitations and the user-facing rendering.
        claim.status = if has_contradiction {
            "contradicted"
        } else if supporting_hosts.len() >= 2 {
            "corroborated"
        } else if !supporting_hosts.is_empty() {
            "single_source"
        } else {
            "unverified"
        }
        .into();
    }
    for contradiction in &result.contradictions {
        if contradiction.description.trim().is_empty()
            || !result
                .claims
                .iter()
                .any(|claim| claim.id == contradiction.claim_id && claim.status == "contradicted")
        {
            return Err("冲突说明必须对应有反对引文的论点".into());
        }
    }
    for claim in result
        .claims
        .iter()
        .filter(|claim| claim.status == "contradicted")
    {
        if !result
            .contradictions
            .iter()
            .any(|item| item.claim_id == claim.id)
        {
            return Err("存在反对证据的论点缺少冲突说明".into());
        }
    }
    for scope in [
        "依据检索摘录整理，未核验网页全文；资料类型与阅读优先级依据可见内容判断。",
        "跨来源支持不等于编辑独立或事实认证，转载关系仍需核对。",
    ] {
        if !result.limitations.iter().any(|item| item == scope) {
            result.limitations.push(scope.into());
        }
    }
    Ok(())
}

pub fn render_markdown(build: &KnowledgeBuild) -> Result<String, String> {
    let result = build.result.as_ref().ok_or("构建结果不可用")?;
    let mut text = format!(
        "# {}\n\n{}\n\n构建 ID：{}\n检索：{} 次查询，{} 条去重来源。\n\n## 知识层级\n\n",
        build.topic,
        result.summary,
        build.id,
        build.progress.queries_completed,
        build.sources.len()
    );
    fn render_node(text: &mut String, result: &BuildResult, parent: Option<&str>, depth: usize) {
        for node in result
            .nodes
            .iter()
            .filter(|node| node.parent_id.as_deref() == parent)
        {
            text.push_str(&format!(
                "{} {} [{}]\n\n{}\n\n来源：{}\n\n",
                "#".repeat((depth + 3).min(6)),
                node.title,
                node.id,
                node.summary,
                node.source_ids.join(", ")
            ));
            render_node(text, result, Some(&node.id), depth + 1);
        }
    }
    render_node(&mut text, result, None, 0);
    if !result.edges.is_empty() {
        text.push_str("## 概念关系\n\n");
    }
    for edge in &result.edges {
        let relation = match edge.relation {
            EdgeRelation::Prerequisite => "前置知识",
            EdgeRelation::Related => "相关",
        };
        text.push_str(&format!("- {} → {}：{}\n", edge.from, edge.to, relation));
    }
    text.push_str("\n## 论点与交叉核对\n\n");
    for claim in &result.claims {
        text.push_str(&format!(
            "### {} [{}]\n\n{}\n\n证据状态：{}\n\n",
            claim.id, build.id, claim.text, claim.status
        ));
        for evidence in &claim.evidence {
            let stance = if evidence.stance == EvidenceStance::Supports {
                "支持"
            } else {
                "存在分歧"
            };
            text.push_str(&format!(
                "- {} [{}]：{}\n",
                stance, evidence.source_id, evidence.quote
            ));
        }
    }
    text.push_str("\n## 已发现的分歧\n\n");
    if result.contradictions.is_empty() {
        text.push_str("当前摘录未发现有引文支持的直接矛盾，不代表不存在分歧。\n");
    }
    for item in &result.contradictions {
        text.push_str(&format!("- [{}] {}\n", item.claim_id, item.description));
    }
    text.push_str("\n## 来源\n\n");
    for source in &build.sources {
        text.push_str(&format!("### [{}] {}\n\n{}\n\n查询：{}\n材料范围：{}\n资料优先级：{}\n类型判断：{}（{}）\n\n{}\n\n", source.id, source.title, source.url, source.query, source.evidence_kind, source.source_priority, source.source_type, source.source_type_reason, source.excerpt));
    }
    text.push_str("## 局限与待确认事项\n\n");
    for item in &result.limitations {
        text.push_str(&format!("- {}\n", item));
    }
    Ok(text)
}
