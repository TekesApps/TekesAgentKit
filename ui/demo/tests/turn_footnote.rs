#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::{
    OutputUsage, TurnActionResult, TurnFootnote, TurnFootnoteFacts, TurnFootnoteIntent,
};
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::ContentEvent,
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Rgba, Theme, proposal};
struct Text(CoreTextShaper, f32);
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for Text {
    fn type_scale(&self) -> f32 {
        self.1
    }
    fn shape_surface(
        &mut self,
        s: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            s,
            &TextStyle::system(role.style().size * self.1),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> Size {
        let p = self.shape_surface(s, r, Some(w));
        Size::new(p.width as f32, p.height as f32)
    }
    fn draw(
        &mut self,
        _: &mut Scene,
        _: &str,
        _: proposal::TextRole,
        _: Rect,
        _: ButtonTextAlign,
        _: Rgba,
    ) {
    }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
        Some(self)
    }
}

use serde_json::json;

use std::{sync::Arc, time::Duration};
use tekes_agent_contract::*;
use tekes_agent_presentation::{ComposerDestination, RunScope, TokenUsage, TranscriptTurn};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{RunBinding, TurnActionDispatcher};
use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::*;
fn turn() -> TranscriptTurn {
    TranscriptTurn {
        id: "turn".into(),
        status: "completed".into(),
        input: "input".into(),
        started_at: Some(10_000.),
        ended_at: Some(13_000.),
        usage: TokenUsage::default(),
        fork_boundary: Some(json!({"atSeq":2})),
        footnote_actions: vec![ThreadActionDefinition {
            id: "forkTurn:turn".into(),
            label: "从此回合分叉".into(),
            icon: "fork".into(),
            interaction: "execute".into(),
            enabled: true,
            disabled_reason: None,
            copy_text: None,
        }],
        live_usage: None,
        reported_usage: None,
    }
}
fn scope() -> RunScope {
    RunScope::main("fixture", "a")
}
fn row(host: &mut ContentHost, id: ComponentId) -> &mut TurnFootnote {
    host.get_mut(id)
        .unwrap()
        .component_mut::<TurnFootnote>()
        .unwrap()
}
fn activate(
    host: &mut ContentHost,
    id: ComponentId,
    button: ComponentId,
) -> Vec<TurnFootnoteIntent> {
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    row(host, id).take_intents(&actions)
}
async fn setup() -> (
    Arc<SharedAgent>,
    Fixture,
    Arc<SessionRuntime>,
    Arc<RunBinding>,
) {
    let fixture = Fixture::default().with_lifecycle();
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "fixture".into(),
            Arc::new(fixture.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), scope())
            .await
            .unwrap(),
    );
    (agent, fixture, runtime, binding)
}
fn mounted(binding: &RunBinding) -> (ContentHost, ComponentId, ComponentId) {
    let snapshot = binding.current_run().unwrap();
    let target = &snapshot.transcript.turns[0];
    assert!(target.footnote_actions[0].enabled);
    let mut footer = TurnFootnote::new(900, binding.scope().clone(), Default::default());
    footer
        .update(
            binding.scope(),
            snapshot.revision,
            snapshot.connection_epoch,
            target,
            true,
        )
        .unwrap();
    let id = footer.id();
    let button = footer.action_id(&target.footnote_actions[0].id).unwrap();
    (footer.into_host().unwrap(), id, button)
}
#[test]
fn facts_keep_unknown_partial_live_and_settled_time_distinct() {
    let mut turn = turn();
    turn.usage.output_tokens = 777;
    assert_eq!(
        TurnFootnoteFacts::read(&turn).output_usage,
        None,
        "legacy zero/default is not a provider report"
    );
    assert_eq!(TurnFootnoteFacts::read(&turn).elapsed_seconds(), Some(3.));
    turn.status = "running".into();
    turn.ended_at = None;
    turn.live_usage = Some(TranscriptLiveUsage { output_tokens: 0 });
    turn.reported_usage = Some(TranscriptReportedUsage {
        output_tokens: 20,
        is_complete: true,
    });
    let facts = TurnFootnoteFacts::read(&turn);
    assert_eq!(
        facts.output_usage,
        Some(OutputUsage::Live { output_tokens: 0 })
    );
    assert_eq!(facts.elapsed_seconds(), None);
    turn.status = "error".into();
    turn.ended_at = Some(9_000.);
    turn.reported_usage.as_mut().unwrap().is_complete = false;
    let facts = TurnFootnoteFacts::read(&turn);
    assert_eq!(
        facts.output_usage,
        Some(OutputUsage::Reported {
            output_tokens: 20,
            complete: false
        })
    );
    assert_eq!(facts.elapsed_seconds(), None);
    assert!(facts.text().contains("error") && facts.text().contains("部分报告"));
}
#[test]
fn source_and_identity_fences_leave_readout_unchanged_and_render_disabled_reasons() {
    let mut turn = turn();
    turn.footnote_actions[0].enabled = false;
    turn.footnote_actions[0].disabled_reason = Some("来源仍有任务".into());
    let mut footer = TurnFootnote::new(901, scope(), Default::default());
    footer.update(&scope(), 2, 2, &turn, true).unwrap();
    let id = footer.id();
    let button = footer.action_id("forkTurn:turn").unwrap();
    assert!(
        footer
            .update(&RunScope::main("fixture", "b"), 3, 2, &turn, true)
            .is_err()
    );
    assert!(footer.update(&scope(), 1, 2, &turn, true).is_err());
    assert!(footer.update(&scope(), 3, 1, &turn, true).is_err());
    let mut other = turn.clone();
    other.id = "other".into();
    assert!(footer.update(&scope(), 3, 2, &other, true).is_err());
    other = turn.clone();
    other.status = "error".into();
    assert!(footer.update(&scope(), 2, 2, &other, true).is_err());
    let mut host = footer.into_host().unwrap();
    assert!(activate(&mut host, id, button).is_empty());
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("来源仍有任务"))
    );
}
#[tokio::test]
async fn original_turn_dispatch_uses_exact_boundary_after_navigation_and_retains_both_drafts() {
    let (agent, fixture, runtime, a) = setup().await;
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    for (source, text) in [(&a, "A 草稿"), (&b, "B 草稿")] {
        source
            .edit_text(
                &ComposerDestination::run(source.scope().clone()),
                text.into(),
            )
            .unwrap();
    }
    let (mut host, id, button) = mounted(&a);
    let intent = activate(&mut host, id, button).pop().unwrap();
    let boundary = intent.turn().fork_boundary.clone().unwrap();
    let request = TurnActionDispatcher::new(agent)
        .prepare(a.clone(), &intent, "stable-turn-rpc".into())
        .unwrap();
    let mut demo = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    let token = demo.begin_sidebar_action(
        SessionAddress {
            endpoint_id: "fixture".into(),
            session_id: "a".into(),
        },
        intent.action().id.clone(),
    );
    demo.select(1).unwrap();
    let outcome = request.execute().await.unwrap();
    assert!(outcome.navigation.is_some());
    assert!(
        demo.apply_turn_action_outcome(&token, &outcome)
            .unwrap()
            .is_none()
    );
    let calls = fixture.mutation_receipts();
    let (method, payload, _) = &calls["stable-turn-rpc"];
    assert_eq!(method, "session.fork");
    assert_eq!(payload["sessionId"], "a");
    assert_eq!(payload["atSeq"], boundary["atSeq"]);
    assert_eq!(a.current_run().unwrap().draft.text, "A 草稿");
    assert_eq!(b.current_run().unwrap().draft.text, "B 草稿");
    assert_eq!(demo.selected(), 1);
    row(&mut host, id).refresh();
    let fresh = row(&mut host, id)
        .action_id(intent.action().id.as_str())
        .unwrap();
    assert_ne!(fresh, button);
    assert!(activate(&mut host, id, button).is_empty());
    assert_eq!(activate(&mut host, id, fresh).len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn duplicate_admission_and_dropped_prepared_requests_never_fork() {
    let (agent, fixture, runtime, a) = setup().await;
    let dispatcher = TurnActionDispatcher::new(agent);
    let (mut host, id, button) = mounted(&a);
    let intent = activate(&mut host, id, button).pop().unwrap();
    let request = dispatcher
        .prepare(a.clone(), &intent, "drop-rpc".into())
        .unwrap();
    assert!(dispatcher.prepare(a, &intent, "duplicate".into()).is_err());
    drop(request);
    row(&mut host, id).refresh();
    assert!(fixture.mutation_receipts().is_empty());
    let fresh = row(&mut host, id)
        .action_id(intent.action().id.as_str())
        .unwrap();
    assert_ne!(fresh, button);
    assert_eq!(activate(&mut host, id, fresh).len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_between_capture_and_dispatch_rejects_without_retry() {
    let (agent, fixture, runtime, a) = setup().await;
    let (mut host, id, button) = mounted(&a);
    let intent = activate(&mut host, id, button).pop().unwrap();
    let request = TurnActionDispatcher::new(agent)
        .prepare(a, &intent, "expired-rpc".into())
        .unwrap();
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    assert!(request.execute().await.is_err());
    assert!(fixture.mutation_receipts().is_empty());
    runtime.stop().await.unwrap();
}
#[test]
fn unknown_outcomes_remain_locked_across_publications_and_epochs() {
    let target = turn();
    let mut footer = TurnFootnote::new(902, scope(), Default::default());
    footer.update(&scope(), 1, 1, &target, true).unwrap();
    let id = footer.id();
    let button = footer.action_id("forkTurn:turn").unwrap();
    let mut host = footer.into_host().unwrap();
    let intent = activate(&mut host, id, button).pop().unwrap();
    assert!(intent.complete(TurnActionResult::Unknown("连接中断".into())));
    row(&mut host, id)
        .update(&scope(), 2, 2, &target, true)
        .unwrap();
    let fresh = row(&mut host, id).action_id("forkTurn:turn").unwrap();
    assert!(activate(&mut host, id, fresh).is_empty());
    assert!(!intent.claim_dispatch());
    assert!(!intent.complete(TurnActionResult::Completed(ThreadActionOutcome::default())));
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("不会自动重试"))
    );
}
#[test]
fn status_updates_keep_pressed_buttons_and_aside_layout_leading() {
    let mut target = turn();
    let mut footer = TurnFootnote::new(903, scope(), Default::default());
    footer.update(&scope(), 1, 1, &target, true).unwrap();
    let id = footer.id();
    let button = footer.action_id("forkTurn:turn").unwrap();
    let mut host = footer.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(40., 20., 500., 600.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let p = host.get(button).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(p));
    target.reported_usage = Some(TranscriptReportedUsage {
        output_tokens: 11,
        is_complete: true,
    });
    row(&mut host, id)
        .update(&scope(), 2, 1, &target, true)
        .unwrap();
    assert_eq!(row(&mut host, id).action_id("forkTurn:turn"), Some(button));
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::DARK.into(),
    )
    .unwrap();
    host.event(&ContentEvent::PointerUp(p));
    let actions = host.drain_actions();
    assert_eq!(row(&mut host, id).take_intents(&actions).len(), 1);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::DARK.into(),
    )
    .unwrap();
    for child in row(&mut host, id).children() {
        assert!((child.frame().min_x() - frame.min_x()).abs() < 0.01);
    }
}

#[tokio::test]
async fn equal_endpoint_names_cannot_substitute_a_different_runtime_authority() {
    let (agent, fixture, runtime, _source) = setup().await;
    let (_other_agent, other_fixture, other_runtime, other_source) = setup().await;
    let (mut host, id, button) = mounted(&other_source);
    let intent = activate(&mut host, id, button).pop().unwrap();
    assert!(
        TurnActionDispatcher::new(agent)
            .prepare(other_source, &intent, "foreign-runtime".into())
            .is_err()
    );
    assert!(fixture.mutation_receipts().is_empty() && other_fixture.mutation_receipts().is_empty());
    runtime.stop().await.unwrap();
    other_runtime.stop().await.unwrap();
}
