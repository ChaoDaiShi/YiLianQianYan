//! Preserve the current execution positions before an intentional exit.
use super::*;
use crate::modules::task::TaskNodeStatus;
use std::sync::atomic::Ordering;

impl TaskWorldRuntime {
    pub(super) fn is_shutting_down(&self) -> bool {
        self.shutdown_requested.load(Ordering::Acquire)
    }

    pub(super) fn ensure_dispatch_open(&self) -> Result<(), TaskWorldRuntimeError> {
        if self.is_shutting_down() {
            return Err(TaskWorldRuntimeError::ShuttingDown);
        }
        Ok(())
    }

    pub(super) fn running_supervisor_nodes(&self, graph_id: &TaskGraphId) -> Vec<TaskNodeId> {
        self.supervisors
            .read()
            .get(graph_id)
            .into_iter()
            .flat_map(TaskSupervisor::node_states)
            .filter(|node| node.status == TaskNodeStatus::Running)
            .map(|node| node.node_id.clone())
            .collect()
    }

    pub(crate) fn pause_active_for_shutdown(
        &self,
        commands: &CommandRouter,
        when: i64,
    ) -> Result<(), String> {
        let control = self.control_dispatch_lock.lock();
        self.shutdown_requested.store(true, Ordering::Release);
        let mut active = self
            .database
            .active_goal_conversation_graph_ids()?
            .into_iter()
            .map(TaskGraphId::new)
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|error| error.to_string())?;
        active.extend(
            self.goal_chat_tokens
                .lock()
                .values()
                .map(|(graph, _)| graph.clone()),
        );
        for (graph, harness) in self.harnesses.read().iter() {
            if harness
                .all_attempts()
                .iter()
                .any(|attempt| attempt.status.is_active())
            {
                active.insert(graph.clone());
            }
        }
        let mut active_commands = Vec::new();
        let supervisors = self.supervisors.read();
        active.retain(|graph| supervisors.contains_key(graph));
        for (graph, supervisor) in supervisors.iter() {
            if supervisor
                .node_states()
                .iter()
                .any(|node| node.status == TaskNodeStatus::Running)
            {
                active.insert(graph.clone());
            }
            if let Some((node, execution)) = active_command(supervisor) {
                active_commands.push((
                    graph.clone(),
                    node.clone(),
                    execution.graph_revision.value(),
                    execution.request_id.clone(),
                ));
            }
        }
        drop(supervisors);

        let mut failures = Vec::new();
        let mut paused = HashSet::new();
        for graph in active {
            let checkpoint = self.execution_control(&graph).and_then(|control| {
                if control.state == TaskExecutionControlState::Running {
                    self.pause_task_locked(&graph, when)
                } else {
                    Ok(control)
                }
            });
            match checkpoint {
                Ok(control) if control.state == TaskExecutionControlState::Paused => {
                    paused.insert(graph);
                }
                Ok(_) => {}
                Err(error) => failures.push(format!("{graph}: {error}")),
            }
        }
        drop(control);

        for (graph, node, revision, request) in active_commands {
            if paused.contains(&graph) {
                if let Err(error) =
                    self.cancel_command(&graph, &node, revision, &request, commands, when)
                {
                    tracing::warn!(%graph, %node, %error, "Task command cancellation was not confirmed at shutdown");
                }
            }
        }
        if failures.is_empty() {
            tracing::info!(
                paused_graphs = paused.len(),
                "Saved active task positions for application shutdown"
            );
            Ok(())
        } else {
            Err(failures.join(", "))
        }
    }
}
