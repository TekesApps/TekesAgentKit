#!/usr/bin/env python3
"""Replay an explicit portable TekesUI candidate in an isolated AgentKit snapshot.

Never edits the default manifests/lockfiles or the candidate source. This proves
receiver integration only, not native input, visual acceptance or a release pin.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(source, manifest):
    for name, expected in manifest['files'].items():
        path = source / name
        if not path.resolve().is_relative_to(source) or any(
            parent.is_symlink() for parent in [path, *path.parents] if parent != source.parent
        ):
            raise ValueError(f'unsafe candidate input: {name}')
        if not path.is_file() or sha(path) != expected:
            raise ValueError(f'candidate hash mismatch: {name}')


def install_readonly_label_host(native):
    old = 'impl TextSurfaceHost for TextBackend {\n'
    assert native.read_text().count(old) == 1
    native.write_text(native.read_text().replace(old, old + '    fn shape_label(\n        &mut self, text: &str, role: proposal::TextRole, width: f32,\n        options: tekes_ui_text::LabelOptions,\n    ) -> Option<Rc<tekes_ui_text::LabelLayout>> {\n        let m = role.style();\n        let style = if proposal::text_family(role.id()) == Some("mono") {\n            TextStyle::named("Menlo", m.size * self.type_scale)\n        } else { TextStyle::system(m.size * self.type_scale) }.with_weight(m.weight);\n        Some(Rc::new(self.shaper.shape_label(text, &style,\n            &ShapeOptions::with_width(f64::from(width)), options)\n            .with_min_line_height(f64::from(m.line_height * self.type_scale))))\n    }\n'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--probe', type=Path, help='Explicit receiver test source; default: Transcript-only probe')
    parser.add_argument('--append-run-probe', action='store_true',
                        help='Append the explicit probe to the unchanged formal RunView transaction probe')
    parser.add_argument('--append-rail-probe', action='store_true',
                        help='Append the unchanged actual-rail probe before the explicit extension probe')
    parser.add_argument('--append-start-probe', action='store_true',
                        help='Also preserve the unchanged #32 Start/main-navigation tests')
    parser.add_argument('--summary-labels', action='store_true',
                        help='Candidate-only Summary header label and existing native text-host adapter')
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--workdir', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--item-navigation-native', action='store_true',
                        help='Build the owned native stable-item diagnostic on the actual RunView')
    parser.add_argument('--turn-start-native', action='store_true',
                        help='Use explicit #32 Start for actual ordinary and Latest native rail clicks')
    parser.add_argument('--turn-rail', action='store_true', help='Enable actual rail with exact original compact actions in the isolated SDK only')
    parser.add_argument('--forwarded-focus', action='store_true',
                        help='Delegate the actual rail wrapper focus-decoration hook to its original CompoundButton (#33 SDK only)')
    parser.add_argument('--composer-padding', action='store_true',
                        help='Bind actual Composer capsules and consult the original #34 intent in the real host IME barrier')
    parser.add_argument('--sidebar-help', action='store_true',
                        help='Declare current query help on original Sidebar rows in an explicit fixed SDK only')
    parser.add_argument('--append-padding-intent-probe', action='store_true',
                        help='Retain the previous actual #34 host-barrier regressions before the new probe')
    args = parser.parse_args()
    if args.summary_labels and args.probe is None:
        parser.error('--summary-labels requires an explicit receiver --probe')
    if args.append_run_probe and args.probe is None:
        parser.error('--append-run-probe requires an explicit receiver --probe')
    if args.item_navigation_native and not args.append_run_probe:
        parser.error('--item-navigation-native requires --append-run-probe')
    if args.turn_rail and not args.append_run_probe:
        parser.error('--turn-rail requires --append-run-probe')
    if args.append_rail_probe and not (args.append_run_probe and args.turn_rail):
        parser.error('--append-rail-probe requires --append-run-probe and --turn-rail')
    if args.append_start_probe and not args.append_rail_probe:
        parser.error('--append-start-probe requires --append-rail-probe')
    if args.turn_start_native and not (args.item_navigation_native and args.turn_rail):
        parser.error('--turn-start-native requires --item-navigation-native and --turn-rail')
    if args.forwarded_focus and not args.turn_rail:
        parser.error('--forwarded-focus requires --turn-rail')
    if args.composer_padding and not args.forwarded_focus:
        parser.error('--composer-padding requires --forwarded-focus')
    if args.sidebar_help and not (args.probe and args.append_run_probe):
        parser.error('--sidebar-help requires an explicit probe and --append-run-probe')
    if args.append_padding_intent_probe and not args.composer_padding:
        parser.error('--append-padding-intent-probe requires --composer-padding')
    repo = Path(__file__).resolve().parent.parent
    source = args.source.resolve()
    manifest = json.loads(args.manifest.read_text())
    verify(source, manifest)
    work = args.workdir.resolve()
    work.mkdir(parents=True, exist_ok=False)
    snapshot = work / 'agentkit'
    snapshot.mkdir()
    names = subprocess.check_output(
        ['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=repo
    ).decode().split('\0')
    inputs = {}
    for name in sorted(set(names)):
        if not name or not (name in ('Cargo.toml', 'Cargo.lock', 'AGENTS.md', 'LICENSE')
                           or name.startswith(('crates/', 'ui/'))):
            continue
        original = repo / name
        if original.is_symlink() or not original.is_file():
            raise ValueError(f'unsafe AgentKit snapshot input: {name}')
        if 'target' in Path(name).parts or '.deps' in Path(name).parts:
            continue
        dest = snapshot / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, dest)
        inputs[name] = sha(original)
    probe = (args.probe or repo / 'scripts/probes/agentkit-reading-candidate.rs').resolve()
    if not probe.is_file():
        raise ValueError('receiver probe must be an existing Rust source')
    shutil.copyfile(probe, snapshot / 'ui/demo/tests/reading_candidate.rs')
    probe_base = None
    if args.append_run_probe:
        probe_base = repo / 'scripts/probes/agentkit-run-preflight-candidate.rs'
        (snapshot / 'ui/demo/tests/reading_candidate.rs').write_bytes(
            probe_base.read_bytes().rstrip() + b'\n\n' + probe.read_bytes())
    probe_rail = None
    if args.append_rail_probe:
        probe_rail = repo / 'scripts/probes/agentkit-turn-rail-candidate.rs'
        (snapshot / 'ui/demo/tests/reading_candidate.rs').write_bytes(
            probe_base.read_bytes().rstrip() + b'\n\n' + probe_rail.read_bytes().rstrip() + b'\n\n' + probe.read_bytes())
    probe_start = None
    if args.append_start_probe:
        probe_start = repo / 'scripts/probes/agentkit-turn-start-candidate.rs'
        prepared = snapshot / 'ui/demo/tests/reading_candidate.rs'
        prepared.write_bytes(prepared.read_bytes().rstrip() + b'\n\n' + probe_start.read_bytes())
    probe_padding = None
    if args.composer_padding:
        probe_padding = repo / 'scripts/probes/agentkit-composer-padding-candidate.rs'
        prepared = snapshot / 'ui/demo/tests/reading_candidate.rs'
        prepared.write_bytes(prepared.read_bytes().rstrip() + b'\n\n' + probe_padding.read_bytes())
    probe_padding_host = None
    if args.append_padding_intent_probe:
        probe_padding_host = repo / 'scripts/probes/agentkit-composer-padding-intent-candidate.rs'
        prepared = snapshot / 'ui/demo/tests/reading_candidate.rs'
        prepared.write_bytes(prepared.read_bytes().rstrip() + b'\n\n' + probe_padding_host.read_bytes())
    cargo = snapshot / 'ui/Cargo.toml'
    cargo.write_text(re.sub(
        r'^(tekes-ui-[\w-]+) = \{ git = "https://github.com/TekesApps/TekesUI.git", rev = "[a-f0-9]+" \}$',
        lambda m: f'{m[1]} = {{ path = {json.dumps(str(source / "crates" / m[1]))} }}',
        cargo.read_text(), flags=re.M))
    lock = snapshot / 'ui/Cargo.lock'
    lock.write_text(re.sub(r'^source = "git\+https://github.com/TekesApps/TekesUI.git[^\n]+\n',
                           '', lock.read_text(), flags=re.M))
    # Candidate-only additive API adaptation. Keep the default public pin source
    # unchanged and retain the exact transformed host for receiver review.
    native = snapshot / 'ui/demo/src/native.rs'
    old = '                invalid: n.semantics.invalid,'
    assert native.read_text().count(old) == 1
    if 'required: n.semantics.required,' not in native.read_text():
        native.write_text(native.read_text().replace(old, old + '\n                required: n.semantics.required,'))
    adaptations = ['AX required forwarded from public semantics']
    overrides = ['ui/Cargo.toml', 'ui/Cargo.lock', 'ui/demo/src/native.rs']
    if args.summary_labels:
        component_manifest = snapshot / 'ui/components/Cargo.toml'
        component_manifest.write_text(component_manifest.read_text() + '\ntekes-ui-text.workspace = true\n')
        lock_text, count = re.subn(
            r'(name = "tekes-agent-components-ui"\n.*?dependencies = \[\n)',
            r'\1 "tekes-ui-text",\n', lock.read_text(), count=1, flags=re.S)
        assert count == 1
        lock.write_text(lock_text)
        # Only section headers are single-line in ClientKit. Ordinary Summary
        # row titles must keep natural wrapping, including passive usage slots.
        section = snapshot / 'ui/components/src/summary_section.rs'
        old = 'Content::new(label, TextContent::new(""))'
        assert section.read_text().count(old) == 1
        section.write_text(section.read_text().replace(old, old + '''
                    .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                        tekes_ui_text::LabelOverflow::TailEllipsis))
                    .expect("Summary header has a passive TextContent title")'''))
        install_readonly_label_host(native)
        shutil.copyfile(repo / 'scripts/probes/agentkit-reading-candidate.rs',
                        snapshot / 'ui/demo/tests/agentkit-reading-candidate.rs')
        shutil.copyfile(repo / 'scripts/probes/agentkit-run-preflight-candidate.rs',
                        snapshot / 'ui/demo/tests/agentkit-run-preflight-candidate.rs')
        native_probe = snapshot / 'ui/demo/src/native_summary_sections_selftest.rs'
        old = '                self.widget = Self::widget(view, row);'
        assert native_probe.read_text().count(old) == 1
        native_probe.write_text(native_probe.read_text().replace(old, '''                let header_ax = view.tree.semantics().into_iter().find(|n| n.id == header).unwrap();
                assert!(header_ax.semantics.name.chars().count() > 300);
                assert!(header_ax.frame.size.height < 2. * tekes_ui_tokens::proposal::TextRole::UiBody.style().line_height);
''' + old))
        old = '                run_script(vec![(100, Action::Close)]);'
        assert native_probe.read_text().count(old) == 1
        native_probe.write_text(native_probe.read_text().replace(old, '''                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SUMMARY_LABEL_CAPTURE") {
                    actions.push((300, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);'''))
        demo = snapshot / 'ui/demo/src/component_demo.rs'
        old = 'SummaryView::new(101 + self.selected as u32 * 2, SummaryLabels::default())'
        assert demo.read_text().count(old) == 1
        demo.write_text(demo.read_text().replace(old, '''SummaryView::new(101 + self.selected as u32 * 2, SummaryLabels {
                            section: |id| format!("{id} · 长标题 English 👩🏽‍💻 e\\u{301} 组合字素 ").repeat(16),
                            ..SummaryLabels::default()
                        })'''))
        overrides.extend(['ui/components/Cargo.toml',
                          'ui/components/src/summary_section.rs', 'ui/demo/src/component_demo.rs',
                          'ui/demo/tests/agentkit-reading-candidate.rs',
                          'ui/demo/tests/agentkit-run-preflight-candidate.rs',
                          'ui/demo/src/native_summary_sections_selftest.rs'])
        adaptations.extend(['Only original Summary header opts into single-line tail ellipsis; ordinary row titles retain natural wrapping',
                            'Existing native TextBackend delegates shape_label to its original CoreTextShaper',
                            'Owned native Summary header supplies a long mixed-language title; existing pointer probe checks its full AX name and row bounds'])
    if args.sidebar_help:
        sidebar = snapshot / 'ui/components/src/sidebar.rs'
        old = '            // CANDIDATE_SIDEBAR_ROW_HELP_DECLARATION'
        assert sidebar.read_text().count(old) == 1
        sidebar.write_text(sidebar.read_text().replace(old, old + '\n            stack.children[0].set_help(input.search_help.as_ref()\n                .filter(|help| help.results_ready && self.search.as_ref() == Some(&help.source))\n                .and_then(|help| help.snippets.get(&row.address)).or(row.folder_path.as_ref()).cloned());'))
        support = snapshot / 'ui/components/src/sidebar_search.rs'
        old = 'pub fn sidebar_row_help_supported() -> bool {\n    false\n}'
        assert support.read_text().count(old) == 1
        support.write_text(support.read_text().replace(old, old.replace('false', 'true')))
        if not args.summary_labels:
            install_readonly_label_host(native)
        native_probe = snapshot / 'ui/demo/src/native_sidebar_query_selftest.rs'
        code = native_probe.read_text()
        old = '    results_capture: Option<std::path::PathBuf>,'
        assert code.count(old) == 1
        code = code.replace(old, old + '\n    help_editor: String,\n    help_focus: Option<(ComponentId, u64)>,\n    help_pointer_events: usize,\n    help_value: String,\n    help_cancel_observed: bool,')
        old = '            results_capture: None,'
        assert code.count(old) == 1
        code = code.replace(old, old + '\n            help_editor: String::new(),\n            help_focus: None,\n            help_pointer_events: 0,\n            help_value: String::new(),\n            help_cancel_observed: false,')
        old = '        // CANDIDATE_SIDEBAR_HELP_NATIVE_ADVANCE'
        assert code.count(old) == 1
        code = code.replace(old, old + '\n        if self.phase >= 10 { self.advance_help(host, window); return; }')
        old = '                // CANDIDATE_SIDEBAR_HELP_NATIVE_BEGIN'
        assert code.count(old) == 1
        code = code.replace(old, old + '\n                if std::env::var_os("AGENT_SIDEBAR_HELP_CAPTURE").is_some() {\n                    let editor = host.view.tree.get(host.view.ids().editor).unwrap().visible().unwrap().center();\n                    let row_point = host.view.tree.get(row).unwrap().visible().unwrap().center();\n                    self.phase = 10;\n                    run_script(vec![(0, Action::Click { x: editor.x as f64, y: editor.y as f64 }),\n                        (100, Action::PreeditRange { text: "中文组字".into(), selection: 1..3, replacement: None }),\n                        (200, Action::MouseMove { x: row_point.x as f64, y: row_point.y as f64 })]);\n                    return;\n                }')
        old = '        // CANDIDATE_SIDEBAR_HELP_DEADLINE'
        assert code.count(old) == 1
        code = code.replace(old, old + '\n        if std::env::var_os("AGENT_SIDEBAR_HELP_CAPTURE").is_some() { return 30000; }')
        owned_probe = repo / 'scripts/probes/agentkit-sidebar-help-native.rs'
        native_probe.write_bytes(code.encode() + b'\n' + owned_probe.read_bytes())
        native_host = snapshot / 'ui/demo/src/components_native.rs'
        old = '                    let commands = self.view.event_with_text(&event, &mut self.text);'
        assert native_host.read_text().count(old) == 1
        native_host.write_text(native_host.read_text().replace(old,
            '                    if let Some(probe) = &mut self.sidebar_query_probe { probe.help_observe_input(&event); }\n' + old))
        overrides.append('ui/demo/src/components_native.rs')
        overrides.append('ui/demo/src/native_sidebar_query_selftest.rs')
        overrides.extend(['ui/components/src/sidebar.rs', 'ui/components/src/sidebar_search.rs'])
        adaptations.extend(['Current atomic Sidebar query help is declared on its original retained Button Content; no added row, action or tooltip engine',
                            'Existing native TextBackend delegates readonly help label shaping to its original CoreTextShaper'])
    if args.forwarded_focus:
        action = snapshot / 'ui/components/src/turn_rail_action.rs'
        old = 'impl Component for TurnRailAction {\n'
        assert action.read_text().count(old) == 1
        action.write_text(action.read_text().replace(old, old + '''    fn focus_decoration_above_siblings(&self) -> bool {
        self.button.focus_decoration_above_siblings()
    }
'''))
        focus_probe = repo / 'scripts/probes/agentkit-forwarded-focus-unit.rs'
        action.write_bytes(action.read_bytes() + b'\n' + focus_probe.read_bytes())
        overrides.append('ui/components/src/turn_rail_action.rs')
        adaptations.append('Actual retained TurnRailAction delegates #33 focus decoration to its original CompoundButton; no copied predicates or ring painter')
    if args.turn_rail:
        rail = snapshot / 'ui/components/src/turn_rail_component.rs'
        old = 'pub fn turn_rail_supported() -> bool {\n    false\n}'
        assert rail.read_text().count(old) == 1
        rail.write_text(rail.read_text().replace(old, old.replace('false', 'true')))
        old = 'Content::new(id, TurnRailAction::new(tick))'
        assert rail.read_text().count(old) == 1
        rail.write_text(rail.read_text().replace(old, old + '.preserving_focus()'))
        action = snapshot / 'ui/components/src/turn_rail_action.rs'
        old = '        // CANDIDATE_COMPACT_ACTION_CONFIGURATION'
        assert action.read_text().count(old) == 1
        action.write_text(action.read_text().replace(old, old + '\n        button.action.button.frame_policy = tekes_ui_controls::ButtonFramePolicy::Exact;\n        button.action.pointer_activation_phase = tekes_ui_controls::composition::ButtonActivationPhase::Press;'))
        demo = snapshot / 'ui/demo/src/component_demo.rs'
        old = '        self.document_roots.insert(ids.root, root);'
        assert demo.read_text().count(old) == 1
        demo.write_text(demo.read_text().replace(old, '        self.tree.get_mut(ComponentId(ids.root.0 + 16)).unwrap().component_mut::<RunView>().unwrap().install_turn_rail(9_000_000 + (ids.root.0 >> 32) as u32).map_err(unsupported)?;\n' + old))
        overrides.extend(['ui/components/src/turn_rail_component.rs', 'ui/components/src/turn_rail_action.rs', 'ui/demo/src/component_demo.rs'])
        adaptations.append('Actual AgentKit rail enabled only with original library Exact/Press and preserving_focus; default fixed pin remains unsupported')
    if args.composer_padding:
        demo = snapshot / 'ui/demo/src/component_demo.rs'
        code = demo.read_text()
        old = '        // CANDIDATE_COMPOSER_PADDING_IME_QUERY'
        assert code.count(old) == 1
        code = code.replace(old, old + '''
        if self.tree.padding_focus_target(event).is_some_and(|target|
            self.composing_editor() == Some(target)) {
            return false;
        }''')
        old = '        // CANDIDATE_COMPOSER_PADDING_BINDING'
        assert code.count(old) == 1
        code = code.replace(old, old + '''
        for ids in [Some(self.ids()), self.panel_composer_ids()].into_iter().flatten() {
            if self.tree.get(ComponentId(ids.root.0 + 16))
                .and_then(|node| node.component::<RunView>()).is_some_and(RunView::is_pane_presented)
                && self.tree.get(ids.root).and_then(|node| node.component::<Composer>())
                    .is_some_and(Composer::is_presented) {
                self.tree.set_padding_focus(ids.input_band, Some(ids.editor))?;
            }
        }''')
        # Receiver-only observation after the real host barrier, before the
        # original dispatch. A swallowed event cannot produce this receipt.
        old = '''        self.tree
            .event_with_text(event, text)
            .map_err(|_| unsupported("demo text input geometry"))?;'''
        assert code.count(old) == 1
        code = code.replace(old, '''        let padding_before = self.tree.padding_focus_target(event)
            .filter(|id| self.composing_editor() == Some(*id))
            .map(|id| (id, self.tree.capture_focus(),
                self.tree.focused_text().unwrap().input.external_baseline(),
                self.tree.focused_text().unwrap().input.marked.clone()));
''' + old + '''
        if let Some((id, focus, baseline, marked)) = padding_before {
            assert_eq!(self.tree.focused(), Some(id));
            assert_eq!(self.tree.capture_focus(), focus);
            assert_eq!(self.tree.focused_text().unwrap().input.external_baseline(), baseline);
            assert_eq!(self.tree.focused_text().unwrap().input.marked, marked);
            eprintln!("COMPOSER_PADDING_IME_ROUTE_OK original-editor+mount+marked-selection+baseline");
        }''')
        demo.write_text(code)
        host = snapshot / 'ui/demo/src/components_native.rs'
        code = host.read_text()
        start = code.index('fn native_script(view: &ComponentDemo)')
        end = code.index('fn side_chat_script(', start)
        script = code[start:end]
        old = 'Action::Preedit("中文组字".into())'
        assert script.count(old) == 1
        script = script.replace(old, 'Action::PreeditRange { text: "中文组字".into(), selection: 1..3, replacement: None }')
        old = '    run_script(script);'
        assert script.count(old) == 1
        script = script.replace(old, '''    let band = view.tree.get(ids.input_band).unwrap().frame();
    let padding = tekes_ui_scene::Point::new(band.min_x() + 2., band.center().y);
    assert!(!view.tree.get(ids.editor).unwrap().frame().contains(padding));
    script.insert(3, (600, Action::Click { x: f64::from(padding.x), y: f64::from(padding.y) }));
    script.insert(4, (650, Action::ExpectTextState { text: "中文组字".into(), selection: 1..3, marked: Some(0..4), query: 1..3, actual: 1..3 }));
''' + old)
        code = code[:start] + script + code[end:]
        start = code.index('fn side_chat_script(view: &ComponentDemo)')
        end = code.index('fn task_script(', start)
        script = code[start:end]
        old = '    run_script(script);'
        assert script.count(old) == 1
        script = script.replace(old, '''    let band = view.tree.get(ids.input_band).unwrap().frame();
    let padding = tekes_ui_scene::Point::new(band.min_x() + 2., band.center().y);
    assert!(!view.tree.get(ids.editor).unwrap().frame().contains(padding));
    script.extend([
        (720, Action::PreeditRange { text: "辅助组字".into(), selection: 1..3, replacement: None }),
        (760, Action::Click { x: f64::from(padding.x), y: f64::from(padding.y) }),
        (780, Action::ExpectTextState { text: "SideChat 暂存草稿辅助组字".into(), selection: 14..16, marked: Some(13..17), query: 14..16, actual: 14..16 }),
        (820, Action::TextRange { text: String::new(), replacement: Some(13..17) }),
    ]);
    script.sort_by_key(|(time, _)| *time);
''' + old)
        host.write_text(code[:start] + script + code[end:])
        overrides.extend(['ui/demo/src/component_demo.rs', 'ui/demo/src/components_native.rs'])
        adaptations.append('Actual main/current auxiliary Composer capsules bind original mounted editor after successful render; #34 same-turn query opens only the composing editor route; original dispatch rechecks; native fixture asserts original mount/baseline/marked selection after a real padding click')
    if args.item_navigation_native:
        native_probe_name = 'ui/demo/src/native_item_navigation_candidate.rs'
        shutil.copyfile(repo / ('scripts/probes/native-turn-rail-candidate.rs' if args.turn_rail else 'scripts/probes/native-item-navigation-candidate.rs'), snapshot / native_probe_name)
        if args.turn_start_native:
            native_probe = snapshot / native_probe_name
            code = native_probe.read_text()
            before = 'fn ordinary_turn_mode() -> bool {\n    false\n}'
            assert code.count(before) == 1
            code = code.replace(before, before.replace('false', 'true'))
            before = '''        RunTurnNavigationKind::TurnStart(_) => {
            // CANDIDATE_NATIVE_START_EXECUTION
            return Err("Start alignment unavailable in this candidate".into());
        }'''
            assert code.count(before) == 1
            code = code.replace(before, '''        RunTurnNavigationKind::TurnStart(item) => {
            host.view.tree.reveal_scroll_reading_item(intent.viewport(), &version, item,
                ScrollRevealOptions { placement: ScrollRevealPlacement::Start(tekes_ui_controls::extended::Axis::Vertical), animated: true, ..Default::default() },
                &mut host.text).map_err(|error| format!("{error:?}"))?;
            4
        }''')
            before = 'Action::Preedit("中文组字".into())'
            assert code.count(before) == 1
            code = code.replace(before, 'Action::PreeditRange { text: "中文组字".into(), selection: 1..3, replacement: None }')
            adaptations.append('Native #32 mode uses original Vertical Start with zero margins, actual ordinary+Latest rail clicks, main-host history hooks and original marked selection1..3')
            native_probe.write_text(code)
        host = snapshot / 'ui/demo/src/components_native.rs'
        code = host.read_text()
        replacements = [
            ('mod summary_sections_probe;', 'mod summary_sections_probe;\n#[path = "native_item_navigation_candidate.rs"]\nmod item_navigation_probe;'),
            ('    summary_sections_probe: Option<summary_sections_probe::Probe>,',
             '    summary_sections_probe: Option<summary_sections_probe::Probe>,\n    item_navigation_probe: Option<item_navigation_probe::Probe>,'),
            ('    let summary_sections_selftest =',
             '    let item_navigation_selftest = args.iter().any(|arg| arg == "--item-navigation-selftest");\n    if item_navigation_selftest { assert!(!args.iter().any(|arg| arg == "--endpoint-config" || arg == "--offscreen" || arg == "--native-selftest"), "item diagnostic requires only the owned local fixture window"); }\n    let summary_sections_selftest ='),
            ('        summary_sections_probe: summary_sections_selftest.then(summary_sections_probe::Probe::new),',
             '        summary_sections_probe: summary_sections_selftest.then(summary_sections_probe::Probe::new),\n        item_navigation_probe: item_navigation_selftest.then(item_navigation_probe::Probe::new),'),
            ('        self.text.atlas.begin_frame();',
             '        if let Some(mut probe) = self.item_navigation_probe.take() { probe.before_render(self, window); self.item_navigation_probe = Some(probe); }\n        self.text.atlas.begin_frame();'),
            ('        window.set_animating(self.text.frame_requested);',
             '        if let Some(mut probe) = self.item_navigation_probe.take() { probe.after_render(self, window); self.item_navigation_probe = Some(probe); }\n        window.set_animating(self.text.frame_requested);'),
            ('        if let Some(probe) = &self.summary_sections_probe {',
             '        if let Some(probe) = &self.item_navigation_probe { assert!(self.failures.is_empty()); println!("NATIVE_AGENT_ITEM_NAVIGATION_RECEIPT {}", probe.receipt(self)); }\n        if let Some(probe) = &self.summary_sections_probe {'),
        ]
        for before, after in replacements:
            assert code.count(before) == 1, before
            code = code.replace(before, after)
        if args.turn_rail:
            before = '''            if let DemoCommand::TurnNavigation { root: _, intent: _ } = command {
                // CANDIDATE_TURN_RAIL_DISPATCH
                self.failure("当前 UI 版本暂不支持此回合导航".into());
                continue;
            }'''
            assert code.count(before) == 1
            code = code.replace(before, '''            if let DemoCommand::TurnNavigation { root, intent } = command {
                if let Err(error) = item_navigation_probe::dispatch(self, root, &intent) { self.failure(error); }
                continue;
            }''')
        if args.turn_rail:
            before = "    fn handle(&mut self, event: Event, window: &mut Window) {\n"
            assert code.count(before) == 1
            code = code.replace(before, before + '''        if self.item_navigation_probe.is_some() && matches!(&event, Event::Mouse(_) | Event::TextUnmark | Event::TextCommit { .. } | Event::TextPreedit { .. } | Event::Focus(_)) { eprintln!("RAIL_NATIVE_EVENT {event:?}"); }
''')
        host.write_text(code)
        overrides.extend([native_probe_name, 'ui/demo/src/components_native.rs'])
        adaptations.append('Owned native diagnostic uses actual RunView, original backend and public Kit capture/claim; second-consumer execution is checked in CLI; only public library reading/reveal calls; synthetic AppKit preedit, not a physical IME; actual latest-rail click only when --turn-rail')
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()),
               CARGO_BUILD_JOBS=os.environ.get('CARGO_BUILD_JOBS', '4'),
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_PROFILE_DEV_INCREMENTAL='false', CARGO_PROFILE_TEST_INCREMENTAL='false')
    base = ['cargo', '+1.95.0']
    common = ['--manifest-path', str(cargo), '--locked', '--offline']
    receipt = dict(candidate_identity=manifest['identity'], candidate_files=len(manifest['files']),
                   portable_manifest_sha256=sha(args.manifest), agentkit_inputs=inputs,
                   probe_sha256=sha(probe), source=str(source), snapshot=str(snapshot),
                   probe_base_sha256=sha(probe_base) if probe_base else None,
                   probe_rail_sha256=sha(probe_rail) if probe_rail else None,
                   probe_start_sha256=sha(probe_start) if probe_start else None,
                   probe_padding_sha256=sha(probe_padding) if probe_padding else None,
                   prepared_probe_sha256=sha(snapshot / 'ui/demo/tests/reading_candidate.rs'),
                   candidate_only_host_adaptations=adaptations,
                   candidate_overrides={name: sha(snapshot / name) for name in
                                        overrides},
                   sidebar_help=args.sidebar_help,
                   native_help_probe_sha256=sha(repo / 'scripts/probes/agentkit-sidebar-help-native.rs') if args.sidebar_help else None,
                   probe_padding_host_sha256=sha(probe_padding_host) if probe_padding_host else None,
                   summary_labels=args.summary_labels,
                   item_navigation_native=args.item_navigation_native,
                   turn_rail=args.turn_rail,
                   turn_start_native=args.turn_start_native,
                   forwarded_focus=args.forwarded_focus,
                   composer_padding=args.composer_padding,
                   focus_probe_sha256=sha(focus_probe) if args.forwarded_focus else None,
                   default_pin_promoted=False, native_acceptance=False, visual_acceptance=False,
                   full_runview_acceptance=False, checks=[])
    try:
        metadata = subprocess.check_output(base + ['metadata', *common, '--format-version', '1'], env=env)
        (work / 'metadata.json').write_bytes(metadata)
        packages = [p for p in json.loads(metadata)['packages'] if p['name'].startswith('tekes-ui-')]
        assert len(packages) == 8, [p['name'] for p in packages]
        assert all(Path(p['manifest_path']).resolve().is_relative_to(source) for p in packages)
        steps = [
            ('tests', ['test', *common, '-p', 'tekes-agent-ui-demo', '--test', 'reading_candidate']),
            ('clippy', ['clippy', *common, '-p', 'tekes-agent-ui-demo', '--test', 'reading_candidate', '--', '-D', 'warnings']),
        ]
        if args.forwarded_focus:
            steps.append(('actual-wrapper', ['test', *common, '-p', 'tekes-agent-components-ui',
                                             '--lib', 'forwarded_focus_candidate_tests']))
        if args.summary_labels or args.sidebar_help or args.item_navigation_native or args.turn_rail:
            steps.extend([
                ('public-consumer', ['test', *common, '-p', 'tekes-agent-ui-consumer-check', '--lib']),
                ('native-build', ['build', *common, '-p', 'tekes-agent-ui-demo']),
            ])
        for label, cmd in steps:
            with (work / f'{label}.log').open('w') as log:
                result = subprocess.run(base + cmd, env=env, stdout=log, stderr=subprocess.STDOUT)
            receipt['checks'].append(dict(check=label, exit_code=result.returncode))
            if result.returncode:
                raise RuntimeError(f'{label} failed; see {work / (label + ".log")}')
    finally:
        verify(source, manifest)
        assert all(sha(repo / name) == digest for name, digest in inputs.items()), 'AgentKit input changed during replay'
        (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(dict(workdir=str(work), checks=receipt['checks'])))


if __name__ == '__main__':
    main()
