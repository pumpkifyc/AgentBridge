use super::schema::{
    PreviewBlock, PreviewBlockKind, PreviewItem, PreviewItemKind, PreviewNotice,
    PreviewNoticeScope, PreviewTimelineV1, PreviewTurn,
};

fn sample_timeline() -> PreviewTimelineV1 {
    PreviewTimelineV1 {
        schema_version: 1,
        source_thread_id: "thread-1".into(),
        cutoff_turn_id: Some("turn-1".into()),
        turns: vec![PreviewTurn {
            id: "turn-1".into(),
            index: 0,
            status: "completed".into(),
            started_at: Some(100),
            completed_at: Some(200),
            items: vec![PreviewItem {
                id: "user-1".into(),
                kind: PreviewItemKind::UserMessage,
                title: "用户".into(),
                status: Some("completed".into()),
                timestamp: Some(100),
                sensitive: false,
                blocks: vec![PreviewBlock {
                    kind: PreviewBlockKind::Text,
                    label: "消息".into(),
                    content: "修复登录流程".into(),
                    sensitive: false,
                    default_expanded: true,
                }],
            }],
        }],
        notices: vec![PreviewNotice {
            code: "unknown_items".into(),
            count: 0,
            message: "未知条目只影响预览，原生分享包仍完整保留。".into(),
            scope: PreviewNoticeScope::DisplayOnly,
        }],
    }
}

#[test]
fn serializes_preview_timeline_v1_with_ordered_turns_and_display_only_notice() {
    let value = serde_json::to_value(sample_timeline()).expect("预览时间线应可序列化");

    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["turns"][0]["items"][0]["kind"], "user_message");
    assert_eq!(value["turns"][0]["items"][0]["blocks"][0]["kind"], "text");
    assert_eq!(value["notices"][0]["scope"], "display_only");
}
