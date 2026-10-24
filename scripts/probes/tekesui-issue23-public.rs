//! Isolated candidate probe only; no product pin change or formal Transcript claim.
use tekes_agent_presentation::{ComposerDestination,RunScope,TranscriptItemKind};
use tekes_agent_contract::PromptMode;
use tekes_ui_controls::{DraftRevision,ExternalSelection,ExternalUndo,ExternalValueResult,TextSurfacePresentation,composition::*};
use tekes_ui_tokens::proposal::TextRole;
use tekes_ui_controls::extended::{Axis,Stack};
use std::time::Duration;
#[tokio::test]
async fn shared_history_and_real_fixture_stream_feed_public_plain_content_with_preserved_selection() {
    let (runtime,controller)=tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let history=controller.snapshot();
    let items:Vec<_>=history.transcript.items.iter().filter(|i|!i.text.is_empty()).take(6).collect();
    assert!(items.iter().any(|i|i.kind==TranscriptItemKind::User));
    assert!(items.iter().any(|i|i.kind==TranscriptItemKind::Assistant));
    let children=items.iter().enumerate().map(|(index,item)|Content::new(ComponentId(index as u64+10),TextFieldContent::read_only_content(&item.id,&item.text,TextRole::UiBody))).collect();
    let host=ContentHost::new(Content::new(ComponentId(1),StackContent {model:Stack::new(Axis::Vertical),flexible_child:None,children})).unwrap();
    for (index,item) in items.iter().enumerate() {
        let field=host.get(ComponentId(index as u64+10)).unwrap().component::<TextFieldContent>().unwrap();
        assert_eq!(field.surface.presentation,TextSurfacePresentation::Content);
        assert!(field.surface.input.read_only);
        assert_eq!(field.surface.input.text,item.text);
    }
    let destination=ComposerDestination::run(RunScope::main("fixture","a"));
    controller.edit_text(&destination,"公开 plain content 的流式接入验证".into()).unwrap();
    let ticket=controller.prepare(&destination,PromptMode::Queue).unwrap();
    controller.dispatch(ticket,None).await.unwrap();
    let mut changes=controller.subscribe();
    let first=tokio::time::timeout(Duration::from_secs(4),async {
        loop {if let Some(item)=controller.snapshot().transcript.items.iter().find(|i|i.is_provisional && !i.text.is_empty()) {return item.clone();}changes.changed().await.unwrap();}
    }).await.unwrap();
    let mut field=TextFieldContent::read_only_content("Assistant",&first.text,TextRole::UiBody);
    let initial=field.surface.input.external_baseline();
    assert!(matches!(field.surface.apply_external_value(&initial,&DraftRevision {identity:first.id.clone(),version:1,payload:first.text.clone()},ExternalSelection::Reset,ExternalUndo::Reset),ExternalValueResult::Applied {..}));
    field.accessibility_action(None,&AccessibilityAction::SetSelection(0..1));
    let selected=field.surface.input.selected_text().to_string();
    let second=tokio::time::timeout(Duration::from_secs(4),async {
        loop {if let Some(item)=controller.snapshot().transcript.items.iter().find(|i|i.is_provisional && i.id==first.id && i.text.len()>first.text.len()) {return item.clone();}changes.changed().await.unwrap();}
    }).await.unwrap();
    let baseline=field.surface.input.external_baseline();
    assert!(matches!(field.surface.apply_external_value(&baseline,&DraftRevision {identity:second.id,version:2,payload:second.text.clone()},ExternalSelection::Preserve,ExternalUndo::Preserve),ExternalValueResult::Applied {..}));
    assert_eq!(field.surface.input.selection(),0..1);
    assert_eq!(field.surface.input.selected_text(),selected);
    assert_eq!(field.surface.input.text,second.text);
    runtime.stop().await.unwrap();
}
