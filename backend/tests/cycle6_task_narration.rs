//! Narrow Cycle 6 v1 tests for deterministic Task narration.

use yilian_backend::interaction::{NarrationRequest, TaskNarrator};
use yilian_backend::task::{TaskExecutionControlState, TaskStatusProjection};

#[test]
fn mixed_task_projection_becomes_natural_product_text_without_runtime_leaks() {
    let projection = TaskStatusProjection {
        graph_id: "task-raw-id".to_string(),
        title: "月度学习计划".to_string(),
        overall_status: "working".to_string(),
        current_activity: Some("正在处理：整理文档".to_string()),
        completed_count: 2,
        running_count: 1,
        waiting_count: 2,
        failed_count: 0,
        attention_required: true,
        current_node_summary: Some("整理文档".to_string()),
        control_state: TaskExecutionControlState::Running,
        generation: 3,
        updated_at: 42,
    };

    let request: NarrationRequest = TaskNarrator::narrate(&projection);
    assert!(request.text.contains("月度学习计划"));
    assert!(request.text.contains("已完成 2 项"));
    assert!(request.text.contains("正在处理：整理文档"));
    assert!(request.text.contains("有 2 项等待处理"));
    assert!(request.text.contains("需要你的确认"));
    for internal in [
        "task-raw-id",
        "working",
        "generation",
        "task.node.running",
        "TargetResolution",
    ] {
        assert!(
            !request.text.contains(internal),
            "leaked internal value: {internal}"
        );
    }
}

#[test]
fn terminal_and_paused_statuses_have_bounded_human_messages() {
    let base = TaskStatusProjection {
        graph_id: "raw-id".to_string(),
        title: "交付任务".to_string(),
        overall_status: "completed".to_string(),
        current_activity: None,
        completed_count: 3,
        running_count: 0,
        waiting_count: 0,
        failed_count: 0,
        attention_required: false,
        current_node_summary: None,
        control_state: TaskExecutionControlState::Running,
        generation: 5,
        updated_at: 100,
    };
    let completed = TaskNarrator::narrate(&base);
    assert_eq!(completed.text, "交付任务已完成，共完成 3 项。");

    let paused = TaskStatusProjection {
        overall_status: "paused".to_string(),
        control_state: TaskExecutionControlState::Paused,
        attention_required: true,
        ..base
    };
    let paused_text = TaskNarrator::narrate(&paused).text;
    assert!(paused_text.contains("已暂停"));
    assert!(paused_text.contains("需要你的确认"));
}
