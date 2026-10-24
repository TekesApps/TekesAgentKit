use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_presentation::TranscriptDocument;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::*;

fn capture(binding: &RunBinding, forged: bool) -> HistoricalInputIntent {
    let run = binding.current_run().unwrap();
    let doc = TranscriptDocument::read(&run.transcript).unwrap();
    let mut source = *doc.sections.iter().flat_map(|s| &s.inputs).next().unwrap();
    let mut item = source.item.clone();
    if forged {
        item.text.push_str(" forged");
    }
    source.item = &item;
    let mut row = InputMessage::new(93001, binding.scope().clone());
    row.configure_historical_input(Some(HistoricalInputControl {
        action: HistoricalInputAction::Edit,
        label: "编辑输入".into(),
    }))
    .unwrap();
    row.update(binding.scope(), run.revision, run.connection_epoch, &source)
        .unwrap();
    let button = row.historical_input_button_id().unwrap();
    let mut host = row.into_host().unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(InputMessageIds::new(93001).root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap()
        .take_historical_input_intents(&actions)
        .pop()
        .unwrap()
}

#[tokio::test]
async fn historical_input_callback_requires_real_capability_original_source_and_epoch() {
    for advertised in [false, true] {
        let fixture = if advertised {
            Fixture::default().with_historical_input()
        } else {
            Fixture::default()
        };
        let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        let a = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap();
        let b = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap();
        assert_eq!(a.historical_input_available(), advertised);
        let intent = capture(&a, false);
        let mut callbacks = vec![];
        let result = a.dispatch_historical_input(&intent, |source| {
            callbacks.push((
                source.scope().clone(),
                source.turn_id().to_owned(),
                source.input().clone(),
                source.action(),
            ));
        });
        assert_eq!(result.is_ok(), advertised);
        assert_eq!(callbacks.len(), usize::from(advertised));
        if advertised {
            assert_eq!(
                callbacks[0],
                (
                    a.scope().clone(),
                    intent.turn_id().into(),
                    intent.input().clone(),
                    HistoricalInputAction::Edit
                )
            );
        }
        assert!(
            a.dispatch_historical_input(&intent.clone(), |_| panic!("duplicate callback"))
                .is_err()
        );
        assert!(
            b.dispatch_historical_input(&capture(&a, false), |_| panic!(
                "foreign binding callback"
            ))
            .is_err()
        );
        assert!(
            a.dispatch_historical_input(&capture(&a, true), |_| panic!("forged source callback"))
                .is_err()
        );
        let before = a.current_run().unwrap();
        let stale = capture(&a, false);
        runtime.reconnect().await.unwrap();
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        assert!(
            a.dispatch_historical_input(&stale, |_| panic!("old epoch callback"))
                .is_err()
        );
        let after = a.current_run().unwrap();
        assert_eq!(before.draft, after.draft, "callback cannot replace a draft");
        assert!(after.submissions.is_empty(), "callback cannot submit");
        runtime.stop().await.unwrap();
    }
}
