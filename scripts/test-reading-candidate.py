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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--probe', type=Path, help='Explicit receiver test source; default: Transcript-only probe')
    parser.add_argument('--append-run-probe', action='store_true',
                        help='Append the explicit probe to the unchanged formal RunView transaction probe')
    parser.add_argument('--summary-labels', action='store_true',
                        help='Candidate-only Summary header label and existing native text-host adapter')
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--workdir', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--item-navigation-native', action='store_true',
                        help='Build the owned native stable-item diagnostic on the actual RunView')
    parser.add_argument('--turn-rail', action='store_true', help='Enable actual rail with exact original compact actions in the isolated SDK only')
    args = parser.parse_args()
    if args.summary_labels and args.probe is None:
        parser.error('--summary-labels requires an explicit receiver --probe')
    if args.append_run_probe and args.probe is None:
        parser.error('--append-run-probe requires an explicit receiver --probe')
    if args.item_navigation_native and not args.append_run_probe:
        parser.error('--item-navigation-native requires --append-run-probe')
    if args.turn_rail and not args.append_run_probe:
        parser.error('--turn-rail requires --append-run-probe')
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
        old = 'impl TextSurfaceHost for TextBackend {\n'
        assert native.read_text().count(old) == 1
        native.write_text(native.read_text().replace(old, old + '''    fn shape_label(
        &mut self, text: &str, role: proposal::TextRole, width: f32,
        options: tekes_ui_text::LabelOptions,
    ) -> Option<Rc<tekes_ui_text::LabelLayout>> {
        let m = role.style();
        let style = if proposal::text_family(role.id()) == Some("mono") {
            TextStyle::named("Menlo", m.size * self.type_scale)
        } else { TextStyle::system(m.size * self.type_scale) }.with_weight(m.weight);
        Some(Rc::new(self.shaper.shape_label(text, &style,
            &ShapeOptions::with_width(f64::from(width)), options)
            .with_min_line_height(f64::from(m.line_height * self.type_scale))))
    }
'''))
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
    if args.item_navigation_native:
        native_probe_name = 'ui/demo/src/native_item_navigation_candidate.rs'
        shutil.copyfile(repo / ('scripts/probes/native-turn-rail-candidate.rs' if args.turn_rail else 'scripts/probes/native-item-navigation-candidate.rs'), snapshot / native_probe_name)
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
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_PROFILE_DEV_INCREMENTAL='false', CARGO_PROFILE_TEST_INCREMENTAL='false')
    base = ['cargo', '+1.95.0']
    common = ['--manifest-path', str(cargo), '--locked', '--offline']
    receipt = dict(candidate_identity=manifest['identity'], candidate_files=len(manifest['files']),
                   portable_manifest_sha256=sha(args.manifest), agentkit_inputs=inputs,
                   probe_sha256=sha(probe), source=str(source), snapshot=str(snapshot),
                   probe_base_sha256=sha(probe_base) if probe_base else None,
                   prepared_probe_sha256=sha(snapshot / 'ui/demo/tests/reading_candidate.rs'),
                   candidate_only_host_adaptations=adaptations,
                   candidate_overrides={name: sha(snapshot / name) for name in
                                        overrides},
                   summary_labels=args.summary_labels,
                   item_navigation_native=args.item_navigation_native,
                   turn_rail=args.turn_rail,
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
        if args.summary_labels or args.item_navigation_native or args.turn_rail:
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
