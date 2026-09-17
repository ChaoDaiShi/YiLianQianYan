use super::{ConversationCandidate, InteractionContext};
use crate::shared::interaction::{
    InteractionInput, InteractionIntent, InteractionTarget, TargetResolution,
};
use serde_json::json;

const MAX_CANDIDATES: usize = 16;

/// Deterministic fast-path classifier for cross-context Task/Conversation
/// interactions.  It deliberately produces decisions only; domain adapters
/// remain responsible for command execution and persistence.
#[derive(Clone, Debug)]
pub struct InteractionRouter {
    max_candidates: usize,
}

impl Default for InteractionRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionRouter {
    pub fn new() -> Self {
        Self {
            max_candidates: MAX_CANDIDATES,
        }
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = max_candidates.clamp(1, MAX_CANDIDATES);
        self
    }

    pub fn resolve(
        &self,
        input: &InteractionInput,
        context: &InteractionContext,
    ) -> super::InteractionDecision {
        if let Err(error) = input.validate() {
            return self.missing(
                InteractionIntent::ConversationTurn,
                format!("invalid interaction input: {error}"),
            );
        }

        let utterance = normalize(&input.utterance);

        if let Some((command, intent, target, parameters)) =
            self.resolve_approval(input, &utterance, context)
        {
            return self.decision_with_parameters(intent, target, Some(command), parameters);
        }

        if let Some(command) = task_command(&utterance) {
            let target = self.resolve_task(input, context, &utterance);
            let intent = if command == "task.status" || command == "task.current" {
                InteractionIntent::Query {
                    name: command.clone(),
                }
            } else {
                InteractionIntent::Command {
                    name: command.clone(),
                }
            };
            let executable = self.executable_command(&target, command);
            return self.decision(intent, target, executable);
        }

        if is_graph_proposal(&utterance) {
            let target = self.resolve_task(input, context, &utterance);
            return self.decision(InteractionIntent::GraphMutationProposal, target, None);
        }

        if is_conversation_continuation(&utterance)
            || is_conversation_switch(&utterance)
            || input.conversational_anchor.is_some()
            || context.conversational_anchor.is_some()
        {
            let target = self.resolve_conversation(input, context, &utterance);
            return self.decision(InteractionIntent::ConversationTurn, target, None);
        }

        self.missing(
            InteractionIntent::ConversationTurn,
            "utterance did not match a bounded fast-path intent",
        )
    }

    fn resolve_task(
        &self,
        input: &InteractionInput,
        context: &InteractionContext,
        utterance: &str,
    ) -> TargetResolution {
        let candidates = context
            .tasks
            .iter()
            .filter(|candidate| task_candidate_matches(candidate, input, context, utterance))
            .take(self.max_candidates)
            .map(|candidate| InteractionTarget::Task {
                graph_id: candidate.graph_id.clone(),
            })
            .collect::<Vec<_>>();

        self.resolve_candidates(candidates, "task target is missing")
    }

    fn resolve_conversation(
        &self,
        input: &InteractionInput,
        context: &InteractionContext,
        utterance: &str,
    ) -> TargetResolution {
        if let Some(explicit_id) = explicit_target_id(utterance, "conversation://") {
            let exists = context
                .conversations
                .iter()
                .any(|candidate| candidate.conversation_id == explicit_id)
                || input
                    .conversational_anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.conversation_id == explicit_id)
                || context
                    .conversational_anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.conversation_id == explicit_id);
            return if exists {
                TargetResolution::Resolved {
                    target: InteractionTarget::Conversation {
                        conversation_id: explicit_id,
                    },
                }
            } else {
                TargetResolution::Missing {
                    reason: "explicit conversation target does not exist in context".to_string(),
                }
            };
        }

        let anchor = input
            .conversational_anchor
            .as_ref()
            .or(context.conversational_anchor.as_ref());
        if is_conversation_continuation(utterance) {
            return anchor.map_or_else(
                || TargetResolution::Missing {
                    reason: "conversation anchor is missing".to_string(),
                },
                |anchor| TargetResolution::Resolved {
                    target: InteractionTarget::Conversation {
                        conversation_id: anchor.conversation_id.clone(),
                    },
                },
            );
        }

        let candidates = context
            .conversations
            .iter()
            .filter(|candidate| conversation_candidate_matches(candidate, utterance))
            .take(self.max_candidates)
            .map(|candidate| InteractionTarget::Conversation {
                conversation_id: candidate.conversation_id.clone(),
            })
            .collect::<Vec<_>>();
        self.resolve_candidates(candidates, "conversation target is missing")
    }

    fn resolve_approval(
        &self,
        input: &InteractionInput,
        utterance: &str,
        context: &InteractionContext,
    ) -> Option<(
        String,
        InteractionIntent,
        TargetResolution,
        serde_json::Value,
    )> {
        let resolution = if is_approval_accept(utterance) {
            "approve"
        } else if is_approval_reject(utterance) {
            "reject"
        } else {
            return None;
        };
        let conversation_id = input
            .conversational_anchor
            .as_ref()
            .or(context.conversational_anchor.as_ref())
            .map(|anchor| anchor.conversation_id.as_str());
        let voice_session_id = input.voice_session_id.as_deref();
        if conversation_id.is_none() && voice_session_id.is_none() {
            return Some((
                "task.approval.resolve".to_string(),
                InteractionIntent::Command {
                    name: "task.approval.resolve".to_string(),
                },
                TargetResolution::Missing {
                    reason: "approval conversation context is missing".to_string(),
                },
                json!({}),
            ));
        }
        let matches = context
            .approvals
            .iter()
            .filter(|candidate| {
                let binding = candidate.conversation_id.as_deref();
                binding == conversation_id || binding == voice_session_id
            })
            .take(self.max_candidates)
            .collect::<Vec<_>>();
        let candidates = matches
            .iter()
            .map(|candidate| InteractionTarget::Approval {
                approval_id: candidate.approval_id.clone(),
            })
            .collect::<Vec<_>>();
        let target = self.resolve_candidates(candidates, "pending approval is missing");
        let store_conversation_id = matches
            .as_slice()
            .first()
            .filter(|_| matches.len() == 1)
            .and_then(|candidate| candidate.store_conversation_id.as_deref());
        Some((
            "task.approval.resolve".to_string(),
            InteractionIntent::Command {
                name: "task.approval.resolve".to_string(),
            },
            target,
            store_conversation_id.map_or_else(
                || json!({}),
                |conversation_id| {
                    json!({
                        "resolution": resolution,
                        "conversation_id": conversation_id,
                    })
                },
            ),
        ))
    }

    fn resolve_candidates(
        &self,
        candidates: Vec<InteractionTarget>,
        missing_reason: &str,
    ) -> TargetResolution {
        match candidates.as_slice() {
            [target] => TargetResolution::Resolved {
                target: target.clone(),
            },
            [] => TargetResolution::Missing {
                reason: missing_reason.to_string(),
            },
            _ => TargetResolution::Ambiguous { candidates },
        }
    }

    fn executable_command(&self, target: &TargetResolution, command: String) -> Option<String> {
        matches!(target, TargetResolution::Resolved { .. }).then_some(command)
    }

    fn decision(
        &self,
        intent: InteractionIntent,
        target: TargetResolution,
        command: Option<String>,
    ) -> super::InteractionDecision {
        let command = if matches!(target, TargetResolution::Resolved { .. }) {
            command
        } else {
            None
        };
        super::InteractionDecision {
            intent,
            target,
            command,
            parameters: serde_json::json!({}),
        }
    }

    fn decision_with_parameters(
        &self,
        intent: InteractionIntent,
        target: TargetResolution,
        command: Option<String>,
        parameters: serde_json::Value,
    ) -> super::InteractionDecision {
        let mut decision = self.decision(intent, target, command);
        decision.parameters = parameters;
        decision
    }

    fn missing(
        &self,
        intent: InteractionIntent,
        reason: impl Into<String>,
    ) -> super::InteractionDecision {
        self.decision(
            intent,
            TargetResolution::Missing {
                reason: reason.into(),
            },
            None,
        )
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(
                    character,
                    '。' | '！' | '？' | '?' | '!' | ',' | '，' | '、' | ':' | '：' | ';' | '；'
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn task_command(utterance: &str) -> Option<String> {
    let command = if matches!(
        utterance,
        "暂停"
            | "暂停任务"
            | "暂停这个任务"
            | "暂停当前任务"
            | "暂停那个任务"
            | "pause"
            | "pausetask"
    ) {
        "task.pause"
    } else if matches!(
        utterance,
        "继续"
            | "继续任务"
            | "继续这个任务"
            | "继续当前任务"
            | "继续那个任务"
            | "resume"
            | "resumetask"
    ) {
        "task.resume"
    } else if matches!(
        utterance,
        "取消"
            | "取消任务"
            | "取消这个任务"
            | "取消当前任务"
            | "取消那个任务"
            | "cancel"
            | "canceltask"
    ) {
        "task.cancel"
    } else if matches!(
        utterance,
        "重试"
            | "重试任务"
            | "重试这个任务"
            | "重试当前任务"
            | "重试那个任务"
            | "retry"
            | "retrytask"
    ) {
        "task.retry"
    } else if matches!(
        utterance,
        "重跑"
            | "重跑任务"
            | "重新运行"
            | "重新运行任务"
            | "重新运行这个任务"
            | "重新运行当前任务"
            | "重新运行那个任务"
            | "重跑那个任务"
            | "rerun"
            | "reruntask"
    ) {
        "task.rerun"
    } else if matches!(
        utterance,
        "现在做到哪了" | "现在做到哪一步了" | "任务状态" | "当前状态" | "status" | "taskstatus"
    ) {
        "task.status"
    } else if matches!(
        utterance,
        "当前任务" | "现在的任务" | "目前任务" | "currenttask" | "current"
    ) {
        "task.current"
    } else {
        return None;
    };
    Some(command.to_string())
}

fn task_candidate_matches(
    candidate: &super::TaskCandidate,
    input: &InteractionInput,
    context: &InteractionContext,
    utterance: &str,
) -> bool {
    if !is_generic_task_reference(utterance) {
        if let Some(active_task) = input
            .active_task
            .as_deref()
            .or(context.active_task.as_deref())
        {
            if candidate.graph_id == active_task {
                return true;
            }
        }
    }
    if let Some(explicit_id) = explicit_target_id(utterance, "task://") {
        return candidate.graph_id == explicit_id;
    }
    // A named task wins over generic words such as "当前" or "那个".  For a
    // generic referent every visible candidate remains eligible so ambiguity
    // is returned instead of silently selecting the first task.
    let title = normalize(&candidate.title);
    (!title.is_empty() && utterance.contains(&title))
        || utterance.contains(&normalize(&candidate.graph_id))
        || is_generic_task_reference(utterance)
}

fn conversation_candidate_matches(candidate: &ConversationCandidate, utterance: &str) -> bool {
    let title = normalize(&candidate.title);
    if title.is_empty() {
        return false;
    }
    if utterance.contains(&title) {
        return true;
    }
    let words = title
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| word.len() >= 2)
        .collect::<Vec<_>>();
    !words.is_empty() && words.iter().all(|word| utterance.contains(word))
}

fn is_generic_task_reference(utterance: &str) -> bool {
    matches!(
        utterance,
        "暂停那个任务"
            | "暂停当前任务"
            | "继续那个任务"
            | "继续当前任务"
            | "取消那个任务"
            | "取消当前任务"
            | "重试那个任务"
            | "重试当前任务"
            | "重跑那个任务"
            | "重跑当前任务"
            | "任务状态"
            | "当前任务"
    )
}

fn explicit_target_id(utterance: &str, prefix: &str) -> Option<String> {
    utterance
        .split_once(prefix)
        .map(|(_, value)| {
            value
                .split(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '。' | '！' | '？' | '?' | '!' | ',' | '，')
                })
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .filter(|value| !value.is_empty())
}

fn is_approval_accept(utterance: &str) -> bool {
    matches!(
        utterance,
        "同意" | "批准" | "确认" | "允许" | "approve" | "yes"
    )
}

fn is_approval_reject(utterance: &str) -> bool {
    matches!(
        utterance,
        "拒绝" | "不同意" | "驳回" | "deny" | "reject" | "no"
    )
}

fn is_conversation_continuation(utterance: &str) -> bool {
    utterance.contains("刚才")
        || utterance.contains("上一个问题")
        || utterance.contains("继续这个问题")
        || utterance.contains("刚刚")
        || utterance.contains("previousconversation")
        || utterance.contains("continueconversation")
}

fn is_conversation_switch(utterance: &str) -> bool {
    utterance.contains("对话")
        || utterance.contains("conversation")
        || utterance.starts_with("切到")
        || utterance.starts_with("切换到")
}

fn is_graph_proposal(utterance: &str) -> bool {
    (utterance.contains("并行") || utterance.contains("parallel"))
        && (utterance.contains("测试")
            || utterance.contains("文档")
            || utterance.contains("test")
            || utterance.contains("document"))
}
