//! Owned-host refresh callback; original notice/button/editor and native input.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{Composer, ComposerNoticeInput, ComposerNoticeState};
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: usize,
    callbacks: usize,
    count: usize,
    disabled_since: Option<Instant>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(18_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            baseline: None,
            editor: 0,
            callbacks: 0,
            count: 0,
            disabled_since: None,
        }
    }
    fn composer(host: &Host) -> &Composer {
        host.view
            .tree
            .get(host.view.ids().root)
            .unwrap()
            .component()
            .unwrap()
    }
    fn composer_mut(host: &mut Host) -> &mut Composer {
        let id = host.view.ids().root;
        host.view.tree.get_mut(id).unwrap().component_mut().unwrap()
    }
    fn source(
        host: &Host,
        revision: u64,
        notice: Option<ComposerNoticeState>,
    ) -> ComposerNoticeInput {
        ComposerNoticeInput {
            connection_epoch: host.view.binding().current_run().unwrap().connection_epoch,
            revision,
            destination: Self::composer(host).destination().unwrap().clone(),
            notice,
            enabled: true,
        }
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let p = host.view.tree.get(id).unwrap().visible().unwrap().center();
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(host.view.editor() as *const _ as usize, self.editor);
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
        assert!(host.view.editor().input.marked.is_some());
        assert_eq!(
            host.view.binding().current_run().unwrap().submissions.len(),
            self.count
        );
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(18),
            "notice deadline phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if Self::composer(host).destination().is_some() => {
                self.editor = host.view.editor() as *const _ as usize;
                self.count = host.view.binding().current_run().unwrap().submissions.len();
                Self::composer_mut(host)
                    .install_notice(0xfff0_0001, Default::default())
                    .unwrap();
                let input = Self::source(
                    host,
                    1,
                    Some(ComposerNoticeState {
                        text: "连接目录暂不可用；请显式刷新，保留当前中文组字与草稿。".into(),
                        can_refresh: true,
                        refreshing: false,
                    }),
                );
                Self::composer_mut(host).update_notice(&input).unwrap();
                self.phase = 1;
            }
            1 if window.is_active() => {
                let geometry = Self::composer(host).layout_geometry().unwrap();
                assert!(geometry.notice.unwrap().max_y() < geometry.input_band.min_y());
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        100,
                        Action::PreeditRange {
                            text: "提示刷新期间中文组字".into(),
                            selection: 2..4,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 2;
            }
            2 if host.view.editor().input.marked.is_some() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                run_script(vec![(
                    0,
                    Self::click(host, Self::composer(host).notice().unwrap().refresh_id()),
                )]);
                self.phase = 3;
            }
            3 => {
                let requests = Self::composer_mut(host)
                    .notice_mut()
                    .unwrap()
                    .take_refreshes();
                if !requests.is_empty() {
                    assert_eq!(requests.len(), 1);
                    let request = &requests[0];
                    Self::composer(host)
                        .notice()
                        .unwrap()
                        .claim_refresh(request)
                        .unwrap();
                    assert!(
                        Self::composer(host)
                            .notice()
                            .unwrap()
                            .claim_refresh(&request.clone())
                            .is_err()
                    );
                    assert_eq!(
                        request.source().destination,
                        *Self::composer(host).destination().unwrap()
                    );
                    self.callbacks += 1;
                    self.unchanged(host);
                    let mut input = request.source().clone();
                    input.revision = 2;
                    input.notice.as_mut().unwrap().refreshing = true;
                    Self::composer_mut(host).update_notice(&input).unwrap();
                    self.phase = 4;
                }
            }
            4 => {
                self.unchanged(host);
                let id = Self::composer(host).notice().unwrap().refresh_id();
                assert!(
                    host.view
                        .tree
                        .semantics()
                        .iter()
                        .any(|n| n.id == id && n.semantics.disabled)
                );
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_COMPOSER_NOTICE_CAPTURE") {
                    actions.push((
                        0,
                        Action::Capture {
                            path: format!("{path}.visible.png"),
                        },
                    ));
                }
                actions.push((100, Self::click(host, id)));
                actions.push((300, Action::WakeRedraw));
                self.disabled_since = Some(Instant::now());
                run_script(actions);
                self.phase = 5;
            }
            5 if self
                .disabled_since
                .is_some_and(|since| since.elapsed() >= Duration::from_millis(250)) =>
            {
                self.unchanged(host);
                assert!(
                    Self::composer_mut(host)
                        .notice_mut()
                        .unwrap()
                        .take_refreshes()
                        .is_empty()
                );
                let input = Self::source(host, 3, None);
                Self::composer_mut(host).update_notice(&input).unwrap();
                self.phase = 6;
            }
            6 => {
                self.unchanged(host);
                assert!(
                    Self::composer(host)
                        .layout_geometry()
                        .unwrap()
                        .notice
                        .is_none()
                );
                if let Ok(path) = std::env::var("AGENT_COMPOSER_NOTICE_CAPTURE") {
                    run_script(vec![(0, Action::Capture { path })]);
                }
                run_script(vec![(100, Action::Unmark), (200, Action::Close)]);
                self.phase = 7;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_COMPOSER_NOTICE_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 7);
        assert_eq!(self.callbacks, 1);
        serde_json::json!({"nativeWindow":true,"noticeAboveCard":true,"originalRefreshControl":true,"capturedSourceCallback":true,"cloneClaimRejected":true,"refreshingDisabled":true,"hiddenNoticeNoGap":true,"originalEditorAndMarkedBaseline":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"pixelIconParity":false})
    }
}
