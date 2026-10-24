//! Owned offscreen Native/Pi/DSH SideChat acceptance through original host controls.
//!
//! This module deliberately exercises only the existing Sidebar, SidePanel,
//! Composer and HostResult paths. It does not create pane authority itself.
use super::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tekes_agent_components_ui::{Composer, SidePanel, SidePanelTabKind};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::{ComposerDestination, RunPhase, RunScope, TranscriptItemKind};
use tekes_agent_ui_bindings::{PaneAccess, RunBinding};
use tekes_ui_controls::composition::{
    AccessibilityAction, ButtonContent, ComponentId, ContentEvent,
};

const ENDPOINTS: [&str; 3] = ["native", "pi", "dsh"];
const CANCEL_INPUT: &str = "ENDPOINT_CANCEL_NATIVE_PROOF";

fn tick(host: &mut Host) {
    host.executor.block_on(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
    });
}

fn sync(host: &mut Host) {
    host.view
        .set_sidebar_host_source(host.sidebar.snapshot())
        .unwrap();
    host.view.sync().unwrap();
    let background = host.view.take_background_commands();
    host.commands(Ok(background));
}

fn check_host(host: &Host) {
    assert!(
        host.failures.is_empty(),
        "multi SideChat host errors: {:?}",
        host.failures
    );
}

fn wait(host: &mut Host, predicate: impl Fn(&Host) -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        host.drain_results(None);
        sync(host);
        check_host(host);
        if predicate(host) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "multi SideChat deadline: {label}"
        );
        tick(host);
    }
}

fn panel(host: &Host) -> &SidePanel {
    host.view
        .tree
        .get(host.view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
}

fn press(host: &mut Host, id: ComponentId) -> Vec<DemoCommand> {
    assert!(
        host.view.tree.get(id).is_some(),
        "original control missing: {id:?}"
    );
    host.view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}

fn local_navigation(host: &mut Host, commands: Vec<DemoCommand>) {
    // Remounting an original pane may schedule its normal read-only feedback
    // request. Keep that callback on the production dispatcher; it is neither
    // a Send nor permission to reopen or manufacture a SideChat.
    assert!(
        commands
            .iter()
            .all(|command| matches!(command, DemoCommand::ReadInitialFeedback(_))),
        "local navigation emitted an unexpected business command"
    );
    host.commands(Ok(commands));
    sync(host);
}

fn focus_editor(host: &mut Host, ids: tekes_agent_components_ui::ComposerIds) {
    assert_eq!(
        host.view.panel_composer_ids(),
        Some(ids),
        "only the active scratch editor may be focused"
    );
    let commands = host
        .view
        .accessibility_action(ids.editor, None, AccessibilityAction::Focus)
        .unwrap();
    local_navigation(host, commands);
    assert_eq!(
        host.view.tree.focused(),
        Some(ids.editor),
        "original editor Focus event must reach the active scratch"
    );
}

fn endpoint_parents(host: &Host) -> BTreeMap<String, Vec<Arc<RunBinding>>> {
    let mut grouped: BTreeMap<String, Vec<Arc<RunBinding>>> = BTreeMap::new();
    for binding in host.view.bindings() {
        grouped
            .entry(binding.scope().endpoint_id.clone())
            .or_default()
            .push(binding.clone());
    }
    for endpoint in ENDPOINTS {
        let parents = grouped.get(endpoint).expect("real endpoint parents");
        assert_eq!(
            parents.len(),
            2,
            "each endpoint must contribute two parents: {endpoint}"
        );
        for binding in parents {
            let scope = binding.scope();
            assert_eq!(
                scope.thread_id, scope.session_id,
                "only ordinary main parent bindings are admitted"
            );
            assert_eq!(
                binding.access(),
                PaneAccess::Ready,
                "parent binding must be ready"
            );
            assert!(
                binding.current_run().is_some(),
                "parent Run must be projected"
            );
        }
    }
    assert_eq!(
        grouped.len(),
        ENDPOINTS.len(),
        "unexpected endpoint in this proof"
    );
    grouped
}

fn select_parent(host: &mut Host, parent: &Arc<RunBinding>) {
    let address = SessionAddress {
        endpoint_id: parent.scope().endpoint_id.clone(),
        session_id: parent.scope().session_id.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        sync(host);
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<tekes_agent_components_ui::Sidebar>()
            .unwrap();
        if let Some(id) = sidebar.row_id(&address) {
            let commands = press(host, id);
            host.commands(Ok(commands));
            sync(host);
            assert!(Arc::ptr_eq(host.view.current_binding().unwrap(), parent));
            return;
        }
        if let Some(more) = sidebar.page_button_id() {
            let commands = press(host, more);
            host.commands(Ok(commands));
        }
        assert!(
            Instant::now() < deadline,
            "original Sidebar row did not appear: {address:?}"
        );
        tick(host);
    }
}

fn take_result(host: &mut Host, matches: impl Fn(&HostResult) -> bool, label: &str) -> HostResult {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match host.results.try_recv() {
            Ok(result) if matches(&result) => return result,
            Ok(result) => host.apply_result(result, None),
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "original host result timed out: {label}"
                );
                tick(host);
            }
        }
        sync(host);
        check_host(host);
    }
}

fn wait_sidechat_run(host: &mut Host, binding: &Arc<RunBinding>, label: &str) {
    wait(
        host,
        |_| {
            binding
                .current_run()
                .is_some_and(|run| !run.transcript.is_stale)
        },
        label,
    );
}

fn user_inputs(binding: &Arc<RunBinding>) -> Vec<String> {
    binding
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|item| item.kind == TranscriptItemKind::User && !item.is_provisional)
        .map(|item| item.text.clone())
        .collect()
}

fn dispatch_send(
    host: &mut Host,
    ids: tekes_agent_components_ui::ComposerIds,
    expected: &Arc<RunBinding>,
) {
    let commands = press(host, ids.send);
    assert_eq!(
        commands.len(),
        1,
        "original Composer Send must dispatch once"
    );
    assert!(
        matches!(&commands[0], DemoCommand::Send { binding, .. } if Arc::ptr_eq(binding, expected)),
        "original Send must carry the mounted scratch authority"
    );
    host.commands(Ok(commands));
}

fn scratch_editor_text(host: &Host, ids: tekes_agent_components_ui::ComposerIds) -> String {
    host.view
        .tree
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .editor()
        .input
        .text
        .clone()
}

fn quiesce_results(host: &mut Host) {
    host.drain_results(None);
    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        sync(host);
        while let Ok(result) = host.results.try_recv() {
            host.apply_result(result, None);
        }
        check_host(host);
        if Instant::now() >= deadline {
            return;
        }
        tick(host);
    }
}

fn navigate_summary(host: &mut Host) {
    let id = panel(host)
        .tab_id(&SidePanelTabKind::Summary)
        .expect("original Summary tab");
    let commands = press(host, id);
    local_navigation(host, commands);
    assert!(
        host.view.panel_composer_ids().is_none(),
        "Summary must unmount scratch Composer"
    );
}

fn open_scratch_tab(
    host: &mut Host,
    scope: &RunScope,
    ids: tekes_agent_components_ui::ComposerIds,
    draft: &str,
) {
    let kind = SidePanelTabKind::SideChat {
        session_id: scope.session_id.clone(),
    };
    let id = panel(host)
        .tab_id(&kind)
        .expect("original retained SideChat tab");
    let commands = press(host, id);
    local_navigation(host, commands);
    assert_eq!(
        host.view.panel_composer_ids(),
        Some(ids),
        "original Composer IDs must be retained"
    );
    assert_eq!(
        scratch_editor_text(host, ids),
        draft,
        "original Composer draft must be restored"
    );
}

pub(super) fn run(host: &mut Host) -> Value {
    let mut actual_endpoint_order = Vec::<String>::new();
    for binding in host.view.bindings() {
        let endpoint = &binding.scope().endpoint_id;
        if actual_endpoint_order.last() != Some(endpoint) {
            actual_endpoint_order.push(endpoint.clone());
        }
    }
    assert_eq!(
        actual_endpoint_order,
        ENDPOINTS.map(str::to_owned),
        "actual host bindings must be ordered Native, Pi, DSH"
    );
    let parents = endpoint_parents(host);
    let ordered = ENDPOINTS
        .into_iter()
        .flat_map(|endpoint| {
            parents[endpoint]
                .iter()
                .cloned()
                .map(move |binding| (endpoint, binding))
        })
        .collect::<Vec<_>>();
    assert_eq!(ordered.len(), 6);
    let initial_primary = host.runtime.inspect(|state| state.endpoint_id.clone());
    assert_eq!(
        initial_primary, "native",
        "real multi-endpoint host must keep Native first"
    );
    assert!(host.side_chat_sources.contains_key("native"));
    assert!(host.side_chat_sources.contains_key("pi"));
    assert!(host.side_chat_sources.contains_key("dsh"));
    assert!(
        !Arc::ptr_eq(&host.side_chat_sources["pi"], &host.side_chats),
        "Pi must use its endpoint map owner, never the first-endpoint compatibility field"
    );

    let mut parent_drafts = BTreeMap::new();
    for (index, (endpoint, binding)) in ordered.iter().enumerate() {
        let draft = format!("多端父会话保留草稿:{endpoint}:{index}");
        binding
            .edit_text(
                &ComposerDestination::run(binding.scope().clone()),
                draft.clone(),
            )
            .unwrap();
        parent_drafts.insert(binding.scope().clone(), draft);
    }
    sync(host);

    let dsh_parent = &parents["dsh"][0];
    select_parent(host, dsh_parent);
    let dsh_owner = host.side_chat_sources.get("dsh").unwrap().clone();
    wait(
        host,
        |_| dsh_owner.support_snapshot() == Some(false),
        "DSH unsupported capability publication",
    );
    assert_eq!(
        dsh_owner.support_snapshot(),
        Some(false),
        "DSH must report unavailable SideChat capability"
    );
    let dsh_create = panel(host).create_side_chat_id();
    assert!(
        host.view
            .tree
            .get(dsh_create)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    let dsh_commands = press(host, dsh_create);
    assert!(
        dsh_commands.is_empty(),
        "disabled DSH Create must dispatch no fork command"
    );
    host.commands(Ok(dsh_commands));
    sync(host);
    assert!(dsh_owner.snapshot().chats.is_empty());

    // Native and Pi run the same full original-control lifecycle. Pi's Create
    // completion is held and delivered after selecting another endpoint.
    let mut endpoint_receipts = Vec::new();
    let mut completed_scratch_histories: Vec<Vec<String>> = Vec::new();
    for endpoint in ["native", "pi"] {
        eprintln!("MULTI_SIDECHAT_PHASE endpoint={endpoint} phase=create");
        let parent = &parents[endpoint][0];
        let endpoint_owner = host.side_chat_sources.get(endpoint).unwrap().clone();
        wait(
            host,
            |_| endpoint_owner.support_snapshot() == Some(true),
            "SideChat capability publication",
        );
        let held_pi_create = endpoint == "pi";
        let unique_input = format!(
            "AGENTKIT_MULTI_SIDECHAT_{}_INPUT{}",
            endpoint.to_ascii_uppercase(),
            if held_pi_create {
                " ENDPOINT_QUEUE_HOLD"
            } else {
                ""
            }
        );
        let create_id = {
            select_parent(host, parent);
            panel(host).create_side_chat_id()
        };
        let commands = press(host, create_id);
        assert_eq!(commands.len(), 1);
        assert!(
            matches!(&commands[0], DemoCommand::CreateSideChat(request) if request.parent() == parent.scope())
        );
        host.commands(Ok(commands));
        let held = take_result(
            host,
            |result| matches!(result, HostResult::SideChatOpened { result: Ok(_), .. }),
            "original Create completion",
        );
        let (request_parent, chat, scratch) = if let HostResult::SideChatOpened {
            request,
            result: Ok(run),
        } = &held
        {
            (
                request.parent().clone(),
                run.chat().clone(),
                run.binding().clone(),
            )
        } else {
            unreachable!()
        };
        assert_eq!(&request_parent, parent.scope());
        let owner = host.side_chat_sources.get(endpoint).unwrap().clone();
        let HostResult::SideChatOpened {
            result: Ok(run), ..
        } = &held
        else {
            unreachable!()
        };
        assert!(owner.owns_run(run));
        assert!(scratch.shares_runtime(host.sidebar.runtime_for(endpoint).unwrap()));
        assert_eq!(chat.parent, *parent.scope());
        assert_eq!(chat.scope.endpoint_id, endpoint);
        assert_ne!(chat.scope.session_id, parent.scope().session_id);
        let late_create_did_not_steal = if held_pi_create {
            let alternate = &parents["native"][0];
            select_parent(host, alternate);
            let focused = host.view.tree.focused();
            let selected = panel(host).displayed_input().unwrap().selected.clone();
            host.apply_result(held, None);
            sync(host);
            assert!(
                host.view
                    .current_binding()
                    .is_some_and(|binding| Arc::ptr_eq(binding, alternate))
            );
            assert_eq!(
                panel(host).displayed_input().unwrap().selected,
                selected,
                "late Pi completion must not change another endpoint's panel selection"
            );
            assert_eq!(
                host.view.tree.focused(),
                focused,
                "late Pi completion must not steal focus"
            );
            assert!(host.view.has_side_chat_run(&chat));
            assert!(host.view.has_side_chat_run_binding(&chat, &scratch));
            true
        } else {
            host.apply_result(held, None);
            sync(host);
            false
        };

        if host
            .view
            .current_binding()
            .is_none_or(|binding| !Arc::ptr_eq(binding, parent))
        {
            select_parent(host, parent);
        }
        let kind = SidePanelTabKind::SideChat {
            session_id: chat.scope.session_id.clone(),
        };
        let tab_id = panel(host)
            .tab_id(&kind)
            .expect("original SideChat tab after Create");
        if panel(host).displayed_input().unwrap().selected.as_ref() != Some(&kind) {
            let commands = press(host, tab_id);
            local_navigation(host, commands);
        }
        let ids = host
            .view
            .panel_composer_ids()
            .expect("original SidePanel RunPort Composer");
        assert!(
            host.view.transcript_content_id(ids).is_some(),
            "original scratch Transcript must mount"
        );
        if held_pi_create {
            // A retained tab selected after a late Create has no automatic
            // editor focus policy. Model the user's explicit editor focus via
            // the SDK's original semantic action before typing.
            focus_editor(host, ids);
        } else {
            assert_eq!(
                host.view.tree.focused(),
                Some(ids.editor),
                "initial original Create must focus its mounted scratch editor"
            );
        }
        wait_sidechat_run(host, &scratch, "initial SideChat projection");

        // Synthetic composition tests the component contract, not physical IME.
        let commands = host
            .view
            .event(&ContentEvent::Preedit {
                text: "中文组字".into(),
                selection: 4..4,
                clauses: vec![],
                replacement: None,
            })
            .unwrap();
        assert!(host.view.composing());
        local_navigation(host, commands);
        assert!(
            press(host, ids.send).is_empty(),
            "Send must be blocked during preedit"
        );
        assert_eq!(host.view.tree.focused(), Some(ids.editor));
        assert!(host.view.composing());
        let commands = host
            .view
            .event(&ContentEvent::Commit {
                text: unique_input.clone(),
                replacement: None,
            })
            .unwrap();
        assert!(!host.view.composing());
        assert_eq!(scratch.current_run().unwrap().draft.text, unique_input);
        local_navigation(host, commands);
        quiesce_results(host);
        dispatch_send(host, ids, &scratch);
        let background_selection = if held_pi_create {
            // The owned provider holds this request before emitting headers.
            // Observe completion while another endpoint is actually selected,
            // rather than merely applying an old acknowledgement after the fact.
            let alternate = &parents["native"][0];
            select_parent(host, alternate);
            assert!(
                !scratch
                    .current_run()
                    .unwrap()
                    .transcript
                    .turns
                    .iter()
                    .any(|turn| turn.input == unique_input && turn.status == "completed"),
                "fixture response completed before the cross-endpoint switch"
            );
            Some((
                host.view.tree.focused(),
                panel(host).displayed_input().unwrap().selected.clone(),
            ))
        } else {
            None
        };
        let held_send_result = if held_pi_create {
            Some(take_result(
                host,
                |result| matches!(result, HostResult::Operation(_)),
                "Pi original Send completion",
            ))
        } else {
            None
        };
        wait(
            host,
            |_| {
                let run = scratch.current_run();
                run.is_some_and(|snapshot| {
                    user_inputs(&scratch)
                        .iter()
                        .filter(|value| *value == &unique_input)
                        .count()
                        == 1
                        && snapshot
                            .run
                            .as_ref()
                            .is_some_and(|run| run.phase == RunPhase::Idle)
                        && snapshot.transcript.turns.iter().any(|turn| {
                            turn.input == unique_input
                                && turn.status == "completed"
                                && snapshot.transcript.items.iter().any(|item| {
                                    item.turn_id == turn.id
                                        && item.kind == TranscriptItemKind::Assistant
                                        && item.is_final
                                        && !item.is_provisional
                                        && item.text.contains("ENDPOINT_REPLY_OK")
                                })
                        })
                })
            },
            "scratch Send and final provider reply",
        );
        let completed_inputs = user_inputs(&scratch);
        assert_eq!(
            completed_inputs
                .iter()
                .filter(|value| *value == &unique_input)
                .count(),
            1
        );
        assert!(host.view.bindings().iter().all(|binding| {
            user_inputs(binding)
                .iter()
                .all(|value| value != &unique_input)
        }));
        assert!(
            completed_scratch_histories
                .iter()
                .all(|history| history.iter().all(|value| value != &unique_input))
        );
        let late_send_delivery_did_not_steal = held_send_result.is_some();
        if let Some(held_send) = held_send_result {
            assert!(
                matches!(&held_send, HostResult::Operation(Ok(()))),
                "Pi Send host dispatch must complete successfully"
            );
            let alternate = &parents["native"][0];
            let (focused, selected) = background_selection.as_ref().unwrap();
            assert!(
                host.view
                    .current_binding()
                    .is_some_and(|binding| Arc::ptr_eq(binding, alternate))
            );
            assert_eq!(host.view.tree.focused(), *focused);
            assert_eq!(&panel(host).displayed_input().unwrap().selected, selected);
            host.apply_result(held_send, None);
            sync(host);
            assert!(
                host.view
                    .current_binding()
                    .is_some_and(|binding| Arc::ptr_eq(binding, alternate))
            );
            assert_eq!(&panel(host).displayed_input().unwrap().selected, selected);
            assert_eq!(
                host.view.tree.focused(),
                *focused,
                "late Pi Send completion must not steal focus"
            );
            select_parent(host, parent);
            let kind = SidePanelTabKind::SideChat {
                session_id: chat.scope.session_id.clone(),
            };
            let tab = panel(host).tab_id(&kind).unwrap();
            let commands = press(host, tab);
            local_navigation(host, commands);
        }
        let sidechat_kind = SidePanelTabKind::SideChat {
            session_id: chat.scope.session_id.clone(),
        };
        if panel(host).displayed_input().unwrap().selected.as_ref() != Some(&sidechat_kind) {
            let tab_id = panel(host).tab_id(&sidechat_kind).unwrap();
            let commands = press(host, tab_id);
            local_navigation(host, commands);
        }

        focus_editor(host, ids);
        eprintln!("MULTI_SIDECHAT_PHASE endpoint={endpoint} phase=cancel");
        // A second, delayed stream proves cancellation was issued against a real run.
        let commands = host
            .view
            .event(&ContentEvent::Commit {
                text: CANCEL_INPUT.into(),
                replacement: None,
            })
            .unwrap();
        local_navigation(host, commands);
        assert_eq!(scratch_editor_text(host, ids), CANCEL_INPUT);
        dispatch_send(host, ids, &scratch);
        wait(
            host,
            |host| {
                scratch.current_run().is_some_and(|snapshot| {
                    snapshot.run.as_ref().is_some_and(|run| run.can_cancel)
                        && host.view.panel_composer_ids() == Some(ids)
                        && host
                            .view
                            .tree
                            .get(ids.cancel)
                            .and_then(|node| node.component::<ButtonContent>())
                            .is_some_and(|button| !button.button.state.disabled)
                        && snapshot.transcript.items.iter().any(|item| {
                            item.kind == TranscriptItemKind::Assistant
                                && item.is_provisional
                                && item.text.contains("CANCEL_PENDING")
                        })
                })
            },
            "actual CANCEL_PENDING stream",
        );
        let cancel_commands = press(host, ids.cancel);
        assert_eq!(cancel_commands.len(), 1);
        assert!(
            matches!(&cancel_commands[0], DemoCommand::Cancel { binding, .. } if Arc::ptr_eq(binding, &scratch))
        );
        host.commands(Ok(cancel_commands));
        wait(
            host,
            |_| {
                scratch.current_run().is_some_and(|snapshot| {
                    snapshot.run.as_ref().is_some_and(|run| !run.can_cancel)
                        && snapshot
                            .transcript
                            .turns
                            .iter()
                            .any(|turn| turn.input == CANCEL_INPUT && turn.status == "aborted")
                })
            },
            "runtime confirmed cancelled turn",
        );
        let cancel_turn_status = scratch
            .current_run()
            .unwrap()
            .transcript
            .turns
            .iter()
            .find(|turn| turn.input == CANCEL_INPUT)
            .map(|turn| turn.status.clone())
            .unwrap();
        assert_eq!(
            cancel_turn_status, "aborted",
            "record raw cancellation turn status"
        );
        // Retire this Send completion before the next deliberately held result.
        host.drain_results(None);

        let ids_before_summary = ids;
        scratch
            .edit_text(
                &ComposerDestination::run(chat.scope.clone()),
                format!("SideChat 草稿:{endpoint}"),
            )
            .unwrap();
        sync(host);
        let scratch_draft = scratch.current_run().unwrap().draft.text.clone();
        navigate_summary(host);
        open_scratch_tab(host, &chat.scope, ids_before_summary, &scratch_draft);
        eprintln!("MULTI_SIDECHAT_PHASE endpoint={endpoint} phase=reconnect");
        let old_epoch = scratch.current_run().unwrap().connection_epoch;
        let runtime = host
            .sidebar
            .runtime_for(endpoint)
            .expect("endpoint runtime from original Sidebar host")
            .clone();
        host.executor.block_on(async {
            runtime.reconnect().await.unwrap();
            runtime.wait_ready(Duration::from_secs(20)).await.unwrap();
        });
        wait(
            host,
            |_| {
                scratch.current_run().is_some_and(|snapshot| {
                    snapshot.connection_epoch > old_epoch && !snapshot.transcript.is_stale
                })
            },
            "fresh endpoint reconnect",
        );
        navigate_summary(host);
        open_scratch_tab(host, &chat.scope, ids_before_summary, &scratch_draft);
        let new_epoch = scratch.current_run().unwrap().connection_epoch;
        assert!(new_epoch > old_epoch);
        assert_eq!(
            user_inputs(&scratch)
                .iter()
                .filter(|value| *value == &unique_input)
                .count(),
            1,
            "reopen after reconnect must preserve exactly one submitted input"
        );
        assert_eq!(scratch.current_run().unwrap().draft.text, scratch_draft);
        let canonical_reopen = owner.pointer(&chat.parent, &chat.scope.session_id).unwrap() == chat;
        assert!(canonical_reopen && host.view.has_side_chat_run_binding(&chat, &scratch));
        let final_scratch_history = user_inputs(&scratch);
        assert_eq!(
            final_scratch_history
                .iter()
                .filter(|value| *value == &unique_input)
                .count(),
            1
        );
        assert_eq!(
            final_scratch_history
                .iter()
                .filter(|value| value.as_str() == CANCEL_INPUT)
                .count(),
            1
        );

        eprintln!("MULTI_SIDECHAT_PHASE endpoint={endpoint} phase=close");
        let close_id = panel(host).close_id();
        let close_commands = press(host, close_id);
        assert_eq!(close_commands.len(), 1);
        assert!(
            matches!(&close_commands[0], DemoCommand::CloseSideChat(expected)
            if expected.scope == chat.scope && expected.parent == chat.parent)
        );
        host.commands(Ok(close_commands));
        let delayed_close = endpoint == "pi";
        if delayed_close {
            let held_close = take_result(
                host,
                |result| matches!(result, HostResult::SideChatClosed { .. }),
                "Pi confirmed close",
            );
            assert!(matches!(
                &held_close,
                HostResult::SideChatClosed { result: Ok(()), .. }
            ));
            let alternate = &parents["native"][0];
            select_parent(host, alternate);
            let focus = host.view.tree.focused();
            let selected = panel(host).displayed_input().unwrap().selected.clone();
            host.apply_result(held_close, None);
            sync(host);
            assert!(
                host.view
                    .current_binding()
                    .is_some_and(|binding| Arc::ptr_eq(binding, alternate))
            );
            assert_eq!(
                panel(host).displayed_input().unwrap().selected,
                selected,
                "late Pi close completion must not alter current endpoint"
            );
            assert_eq!(
                host.view.tree.focused(),
                focus,
                "late Pi close completion must not steal current focus"
            );
        } else {
            let close_result = take_result(
                host,
                |result| matches!(result, HostResult::SideChatClosed { .. }),
                "Native confirmed close",
            );
            assert!(matches!(
                &close_result,
                HostResult::SideChatClosed { result: Ok(()), .. }
            ));
            host.apply_result(close_result, None);
            sync(host);
        }
        assert!(owner.pointer(&chat.parent, &chat.scope.session_id).is_err());
        assert_eq!(scratch.access(), PaneAccess::Missing);
        assert!(!host.view.has_side_chat_run(&chat));
        completed_scratch_histories.push(final_scratch_history);
        for (scope, expected) in &parent_drafts {
            let binding = host
                .view
                .bindings()
                .iter()
                .find(|binding| binding.scope() == scope)
                .unwrap();
            assert_eq!(
                binding.current_run().unwrap().draft.text,
                *expected,
                "SideChat lifecycle must preserve every parent draft"
            );
            assert!(
                user_inputs(binding)
                    .iter()
                    .all(|input| input != &unique_input && input.as_str() != CANCEL_INPUT),
                "neither normal nor cancelled scratch input may enter any parent history"
            );
        }
        endpoint_receipts.push(json!({"endpointId":endpoint,"parentScope":parent.scope(),
            "scratchScope":chat.scope,"capabilityUnavailable":false,
            "originalCreate":true,"originalHostDispatch":true,"originalSend":true,
            "runtimeCancelObserved":true,"cancelTurnStatus":cancel_turn_status,
            "parentHistoryIsolated":true,"parentAndOtherDraftsPreserved":true,
            "summaryNavigation":true,"originalComposerIdsRetained":true,"scratchDraftRestored":true,
            "canonicalReopen":true,"freshReconnect":true,"noDuplicateInput":true,
            "originalClose":true,"confirmedDiscard":true,"retiredAuthority":true,
            "lateCreateDeliveryDidNotStealSelection":late_create_did_not_steal,
            "lateSendDeliveryDidNotStealSelection":late_send_delivery_did_not_steal,
            "sendCompletedWhileOtherEndpointSelected":background_selection.is_some(),
            "explicitSdkEditorFocus":true,"automaticTabRefocusProven":false,
            "lateCloseDeliveryDidNotAffectCurrent":delayed_close,
            "scratchInputCount":1,"cancelInputCount":1,"oldEpoch":old_epoch,"newEpoch":new_epoch,
            "uniqueInput":unique_input}));
    }
    let dsh = json!({"endpointId":"dsh","parentScope":parents["dsh"][0].scope(),
        "scratchScope":Value::Null,"capabilityUnavailable":true,"originalCreateDisabled":true,
        "noForkAttempted":true,"parentAndOtherDraftsPreserved":true});
    json!({"accepted":true,"endpointOrder":actual_endpoint_order,"hostFirstEndpoint":initial_primary,
        "cases":[endpoint_receipts[0].clone(),endpoint_receipts[1].clone(),dsh],
        "nativeInteraction":false,"physicalIme":false,"desktopVisibilityProven":false,"crossProcess":false})
}
