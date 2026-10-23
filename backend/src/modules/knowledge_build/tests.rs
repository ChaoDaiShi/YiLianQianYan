use super::*;

mod http;

fn sources() -> Vec<BuildSource> {
    vec![
        BuildSource {
            id: "s1".into(),
            title: "教材章节".into(),
            url: "https://textbook.example/chapter".into(),
            query: "栈 教材".into(),
            excerpt: "栈遵循后进先出原则。".into(),
            evidence_kind: "snippet".into(),
            source_type: "textbook".into(),
            source_priority: "preferred".into(),
            source_type_reason: "摘录自教材章节".into(),
        },
        BuildSource {
            id: "s2".into(),
            title: "课程讲义".into(),
            url: "https://course.example/stack".into(),
            query: "栈 讲义".into(),
            excerpt: "栈是一种后进先出的结构。".into(),
            evidence_kind: "snippet".into(),
            source_type: "reference".into(),
            source_priority: "supporting".into(),
            source_type_reason: "摘录自课程讲义".into(),
        },
    ]
}

fn result() -> BuildResult {
    BuildResult {
        summary: "有来源的栈知识结构".into(),
        nodes: vec![
            KnowledgeNode {
                id: "n1".into(),
                parent_id: None,
                title: "数据结构".into(),
                summary: "数据结构中的栈".into(),
                source_ids: vec!["s1".into()],
            },
            KnowledgeNode {
                id: "n2".into(),
                parent_id: Some("n1".into()),
                title: "栈".into(),
                summary: "后进先出结构".into(),
                source_ids: vec!["s1".into(), "s2".into()],
            },
        ],
        edges: vec![],
        claims: vec![KnowledgeClaim {
            id: "c1".into(),
            text: "栈遵循后进先出".into(),
            status: "model_claimed_verified".into(),
            evidence: vec![
                ClaimEvidence {
                    source_id: "s1".into(),
                    quote: "栈遵循后进先出原则。".into(),
                    stance: EvidenceStance::Supports,
                },
                ClaimEvidence {
                    source_id: "s2".into(),
                    quote: "栈是一种后进先出的结构。".into(),
                    stance: EvidenceStance::Supports,
                },
            ],
        }],
        contradictions: vec![],
        limitations: vec![],
    }
}

#[test]
fn evidence_status_is_derived_and_same_host_is_not_cross_source_confirmation() {
    let mut result = result();
    let mut sources = sources();
    evidence::validate_result(&mut result, &sources).unwrap();
    assert_eq!(result.claims[0].status, "corroborated");
    sources[1].url = "https://www.textbook.example/other".into();
    evidence::validate_result(&mut result, &sources).unwrap();
    assert_eq!(result.claims[0].status, "single_source");
    result.claims[0].evidence.clear();
    evidence::validate_result(&mut result, &sources).unwrap();
    assert_eq!(result.claims[0].status, "unverified");
}

#[test]
fn fabricated_quotes_and_title_only_evidence_are_rejected() {
    let mut fabricated = result();
    fabricated.claims[0].evidence[0].quote = "未经检索的编造事实".into();
    assert!(evidence::validate_result(&mut fabricated, &sources()).is_err());
    let mut title_only = sources();
    title_only[0].evidence_kind = "title_only".into();
    assert!(evidence::validate_result(&mut result(), &title_only).is_err());
}

#[test]
fn missing_nodes_cycles_and_unsupported_conflict_descriptions_are_rejected() {
    let mut cyclic = result();
    cyclic.nodes[0].parent_id = Some("n2".into());
    assert!(evidence::validate_result(&mut cyclic, &sources()).is_err());
    let mut missing = result();
    missing.nodes[1].source_ids = vec!["invented-source".into()];
    assert!(evidence::validate_result(&mut missing, &sources()).is_err());
    let mut contradiction = result();
    contradiction.contradictions.push(Contradiction {
        claim_id: "c1".into(),
        description: "没有反对引文支持的冲突".into(),
    });
    assert!(evidence::validate_result(&mut contradiction, &sources()).is_err());
}

#[test]
fn contradictory_evidence_requires_a_linked_explanation() {
    let mut result = result();
    let mut sources = sources();
    sources[1].excerpt = "该结构不是后进先出。".into();
    result.claims[0].evidence[1] = ClaimEvidence {
        source_id: "s2".into(),
        quote: "该结构不是后进先出。".into(),
        stance: EvidenceStance::Contradicts,
    };
    assert!(evidence::validate_result(&mut result, &sources).is_err());
    result.contradictions.push(Contradiction {
        claim_id: "c1".into(),
        description: "来源对所描述结构的行为有分歧，需确认对象是否一致。".into(),
    });
    evidence::validate_result(&mut result, &sources).unwrap();
    assert_eq!(result.claims[0].status, "contradicted");
}

#[test]
fn deep_round_queries_must_actually_expand_research() {
    assert_eq!(BuildMode::Quick.rounds(), 1);
    assert_eq!(BuildMode::Deep.rounds(), 3);
    assert_eq!(BuildMode::Quick.source_limit(), 12);
    assert_eq!(BuildMode::Deep.source_limit(), 40);
    let previous = vec!["栈 官方定义".into()];
    assert!(parse_queries(
        vec![" 栈 官方定义 ".into(), "B".into(), "C".into(), "D".into()],
        &previous
    )
    .is_err());
    assert!(parse_queries(vec!["A".into(), "a".into(), "C".into(), "D".into()], &[]).is_err());
    assert_eq!(
        parse_queries(
            vec!["A".into(), "B".into(), "C".into(), "D".into()],
            &previous
        )
        .unwrap()
        .len(),
        4
    );
}

fn search_hit(url: String) -> crate::modules::cloud::search::SearchResult {
    crate::modules::cloud::search::SearchResult {
        title: "资料".into(),
        url,
        snippet: Some("真实返回的资料摘录".into()),
        content: None,
    }
}

#[test]
fn oversized_search_responses_reserve_new_sources_for_every_deep_query_and_round() {
    let mut build = KnowledgeBuild::new("数据结构".into(), BuildMode::Deep);
    let mut urls = HashSet::new();
    for (round, cumulative_limit) in [13, 26, 40].into_iter().enumerate() {
        for query_index in 0..4 {
            let query = format!("round-{round}-query-{query_index}");
            let budget = query_source_budget(build.mode, round, query_index, build.sources.len());
            assert!(budget > 0, "later queries must retain collection capacity");
            let hits = (0..10)
                .map(|index| {
                    search_hit(format!(
                        "https://sources.example/{round}/{query_index}/{index}"
                    ))
                })
                .collect();
            collect_sources(&mut build, &mut urls, &query, hits, budget);
            assert_eq!(
                build
                    .sources
                    .iter()
                    .filter(|source| source.query == query)
                    .count(),
                budget
            );
            assert!(build.sources.len() <= cumulative_limit);
        }
        assert_eq!(build.sources.len(), cumulative_limit);
    }
    assert_eq!(urls.len(), 40);
}

#[test]
fn duplicate_and_invalid_sources_do_not_consume_query_budget() {
    let mut build = KnowledgeBuild::new("数据结构".into(), BuildMode::Deep);
    let mut urls = HashSet::new();
    collect_sources(
        &mut build,
        &mut urls,
        "initial",
        vec![search_hit("https://sources.example/known".into())],
        1,
    );
    let hits = vec![
        search_hit("https://sources.example/known#one".into()),
        search_hit("https://sources.example/known#two".into()),
        search_hit("ftp://sources.example/not-web-evidence".into()),
        search_hit("https://sources.example/new-one".into()),
        search_hit("https://sources.example/new-one#duplicate".into()),
        search_hit("https://sources.example/new-two".into()),
        search_hit("https://sources.example/new-three".into()),
    ];
    collect_sources(&mut build, &mut urls, "cross-check", hits, 3);
    assert_eq!(build.sources.len(), 4);
    assert_eq!(
        build
            .sources
            .iter()
            .filter(|source| source.query == "cross-check")
            .count(),
        3
    );
    assert_eq!(urls.len(), 4);
}

#[test]
fn unused_query_slots_are_redistributed_without_using_later_round_reservations() {
    let mut build = KnowledgeBuild::new("数据结构".into(), BuildMode::Deep);
    let mut urls = HashSet::new();
    assert_eq!(query_source_budget(build.mode, 0, 0, 0), 4);
    collect_sources(
        &mut build,
        &mut urls,
        "sparse",
        vec![search_hit("https://sources.example/only-one".into())],
        4,
    );
    for query_index in 1..4 {
        let budget = query_source_budget(build.mode, 0, query_index, build.sources.len());
        assert_eq!(budget, 4);
        let hits = (0..10)
            .map(|index| {
                search_hit(format!(
                    "https://sources.example/fill/{query_index}/{index}"
                ))
            })
            .collect();
        collect_sources(&mut build, &mut urls, "fill-gap", hits, budget);
    }
    assert_eq!(build.sources.len(), 13);
    assert_eq!(
        query_source_budget(build.mode, 1, 0, build.sources.len()),
        4
    );
    assert_eq!(query_source_budget(BuildMode::Quick, 0, 0, 0), 3);
}

#[test]
fn cancellation_and_startup_interruption_cannot_be_overwritten_by_workers() {
    let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
    db.initialize_knowledge_build_schema().unwrap();
    let mut first = KnowledgeBuild::new("数据结构".into(), BuildMode::Deep);
    db.create_knowledge_build(&first).unwrap();
    let cancelled = db.cancel_knowledge_build(&first.id).unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");
    first.status = "completed".into();
    assert!(!db.update_active_knowledge_build(&first).unwrap());
    assert_eq!(
        db.knowledge_build(&first.id).unwrap().unwrap().status,
        "cancelled"
    );
    let mut second = KnowledgeBuild::new("算法".into(), BuildMode::Quick);
    second.sources = sources();
    db.create_knowledge_build(&second).unwrap();
    assert_eq!(db.interrupt_knowledge_builds().unwrap(), 1);
    assert!(!db.update_active_knowledge_build(&second).unwrap());
    let interrupted = db.knowledge_build(&second.id).unwrap().unwrap();
    assert_eq!(interrupted.status, "interrupted");
    assert_eq!(interrupted.sources.len(), 2);
    assert_eq!(db.interrupt_knowledge_builds().unwrap(), 0);
}

#[test]
fn imported_document_retains_hierarchy_evidence_and_provenance() {
    let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
    db.initialize_knowledge_build_schema().unwrap();
    let mut build = KnowledgeBuild::new("数据结构".into(), BuildMode::Quick);
    build.status = "completed".into();
    build.sources = sources();
    let mut output = result();
    evidence::validate_result(&mut output, &build.sources).unwrap();
    build.result = Some(output);
    db.create_knowledge_build(&build).unwrap();
    let markdown = render_markdown(&build).unwrap();
    assert!(markdown.contains("#### 栈 [n2]"));
    assert!(markdown.contains("https://textbook.example/chapter"));
    assert!(markdown.contains("栈遵循后进先出原则。"));
    let (document, created) = db
        .import_knowledge(
            "res-fixture",
            "数据结构",
            "text/markdown",
            "fixture-hash",
            markdown.len() as i64,
            &markdown,
        )
        .unwrap();
    assert!(created);
    db.set_knowledge_build_document(&build.id, &document.id)
        .unwrap();
    assert_eq!(
        db.knowledge_build_for_document(&document.id).unwrap(),
        Some(build.id.clone())
    );
    let (same, created) = db
        .import_knowledge(
            "res-fixture",
            "数据结构",
            "text/markdown",
            "fixture-hash",
            markdown.len() as i64,
            &markdown,
        )
        .unwrap();
    assert!(!created);
    assert_eq!(document.id, same.id);
    assert_eq!(
        db.knowledge_build(&build.id).unwrap().unwrap().document_id,
        Some(document.id.clone())
    );
    assert_eq!(db.knowledge_text(&document.id).unwrap().unwrap(), markdown);
    assert!(!db.search_knowledge("后进先出").unwrap().is_empty());
}
