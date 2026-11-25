use std::path::Path;

use tokio_util::sync::CancellationToken;
use yilian_backend::{
    db::Database,
    modules::task::{
        TaskExecutionControlState, TaskGraphId, TaskWorldRuntime, TaskWorldRuntimeError,
    },
    shared::event::EventHub,
};

fn runtime() -> (Database, TaskWorldRuntime, TaskGraphId, TaskGraphId) {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    let target = TaskGraphId::new("shared-goal").unwrap();
    let other = TaskGraphId::new("other-goal").unwrap();
    for graph in [&target, &other] {
        runtime
            .create_graph(graph.clone(), vec![], vec![], 1)
            .unwrap();
    }
    (db, runtime, target, other)
}

#[test]
fn cancelling_a_goal_stops_all_its_chat_tokens_and_rejects_resume_after_restart() {
    let (db, runtime, target, other) = runtime();
    let first = CancellationToken::new();
    let resumed_approval = CancellationToken::new();
    let unrelated = CancellationToken::new();
    let _first = runtime.register_goal_chat(&target, first.clone()).unwrap();
    let _resumed = runtime
        .register_goal_chat(&target, resumed_approval.clone())
        .unwrap();
    let _other = runtime
        .register_goal_chat(&other, unrelated.clone())
        .unwrap();

    runtime.cancel_task(&target, 2).unwrap();

    assert!(first.is_cancelled());
    assert!(resumed_approval.is_cancelled());
    assert!(!unrelated.is_cancelled());
    assert_eq!(
        runtime.get_graph_detail(&target).unwrap().control_state,
        TaskExecutionControlState::Cancelled
    );
    assert!(matches!(
        runtime.register_goal_chat(&target, CancellationToken::new()),
        Err(TaskWorldRuntimeError::ExecutionCancelled(_))
    ));
    let reopened = TaskWorldRuntime::new(&db, EventHub::new(16)).unwrap();
    assert_eq!(
        reopened.get_graph_detail(&target).unwrap().control_state,
        TaskExecutionControlState::Cancelled
    );
    assert!(matches!(
        reopened.register_goal_chat(&target, CancellationToken::new()),
        Err(TaskWorldRuntimeError::ExecutionCancelled(_))
    ));
}

#[test]
fn pause_stops_chat_and_resume_requires_a_fresh_chat_registration() {
    let (_db, runtime, target, _) = runtime();
    let token = CancellationToken::new();
    let lease = runtime.register_goal_chat(&target, token.clone()).unwrap();
    runtime.pause_task(&target, 2).unwrap();
    assert!(token.is_cancelled());
    assert_eq!(
        runtime.get_graph_detail(&target).unwrap().control_state,
        TaskExecutionControlState::Paused
    );
    assert!(matches!(
        runtime.register_goal_chat(&target, CancellationToken::new()),
        Err(TaskWorldRuntimeError::ExecutionPaused(_))
    ));
    drop(lease);
    runtime.resume_task(&target, 3).unwrap();
    let fresh = CancellationToken::new();
    let fresh_lease = runtime.register_goal_chat(&target, fresh.clone()).unwrap();
    assert!(!fresh.is_cancelled());
    drop(fresh_lease);
    runtime.cancel_task(&target, 4).unwrap();
    assert!(
        !fresh.is_cancelled(),
        "finished chats are no longer registered"
    );
}

#[test]
fn registration_racing_goal_cancel_never_leaves_a_live_chat_signal() {
    let (_db, runtime, _, _) = runtime();
    for iteration in 0..8 {
        let graph = TaskGraphId::new(format!("cancel-race-{iteration}")).unwrap();
        runtime
            .create_graph(graph.clone(), vec![], vec![], 1)
            .unwrap();
        let barrier = std::sync::Barrier::new(2);
        let token = CancellationToken::new();
        std::thread::scope(|scope| {
            let registration = scope.spawn(|| {
                barrier.wait();
                runtime.register_goal_chat(&graph, token.clone())
            });
            barrier.wait();
            runtime.cancel_task(&graph, 2).unwrap();
            match registration.join().unwrap() {
                Ok(_lease) => assert!(token.is_cancelled()),
                Err(TaskWorldRuntimeError::ExecutionCancelled(_)) => {}
                Err(error) => panic!("unexpected registration error: {error}"),
            }
        });
    }
}
