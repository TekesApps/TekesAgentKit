//! Phase 0 safe `AppWindow` launcher bridging `RunSurface` to `tekes-ui-app-host` with `WindowWake`.
//! Zero `unsafe` code; Metal rendering, CoreText, and macOS AX live strictly in TekesUI.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tekes_agent_apphost::AppHostWakeHub;
use tekes_agent_components_ui::{Intent, RunSurface};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::UiWakeNotifier;
use tekes_ui_app_host::{AppWindow, ContainerContent, LocalContainer, PresentationOptions};
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};
use tekes_ui_platform_macos::WindowWake;
use tekes_ui_render::Renderer;
use tekes_ui_tokens::Theme;

struct RunSurfaceContainer {
    surface: Rc<RefCell<RunSurface>>,
    controller: Arc<RunController>,
    wake_hub: Arc<AppHostWakeHub>,
    last_revision: u64,
    active: Rc<Cell<bool>>,
}

impl ContainerContent for RunSurfaceContainer {
    fn with_host(&mut self, visit: &mut dyn FnMut(&mut ContentHost)) {
        let mut surface = self.surface.borrow_mut();
        visit(&mut surface.tree);
    }

    fn prepare_for_render(&mut self) {
        if !self.active.get() {
            return;
        }
        let snapshot = self.controller.snapshot();
        if snapshot.revision != self.last_revision {
            self.last_revision = snapshot.revision;
            let _ = self.surface.borrow_mut().sync(&snapshot);
        }
    }

    fn start_process(&mut self, wake: WindowWake) -> anyhow::Result<()> {
        self.wake_hub.register(wake);
        Ok(())
    }
}

fn dispatch_intents(
    intents: Vec<Intent>,
    controller: &Arc<RunController>,
    runtime: &Arc<SessionRuntime>,
    executor: &tokio::runtime::Runtime,
    scopes: &[RunScope; 2],
    wake_hub: &Arc<AppHostWakeHub>,
) {
    for intent in intents {
        match intent {
            Intent::Edit { destination, text } => {
                let mut draft = controller.snapshot().draft.clone();
                draft.text = text;
                let _ = controller.edit(&destination, draft);
            }
            Intent::Send { destination } => {
                if let Ok(ticket) = controller.prepare(&destination, PromptMode::Queue) {
                    let c = Arc::clone(controller);
                    let w = Arc::clone(wake_hub);
                    executor.spawn(async move {
                        let _ = c.dispatch(ticket, None).await;
                        w.wake();
                    });
                }
            }
            Intent::Cancel { scope } => {
                let c = Arc::clone(controller);
                let w = Arc::clone(wake_hub);
                executor.spawn(async move {
                    let _ = c.cancel(&scope).await;
                    w.wake();
                });
            }
            Intent::Earlier { scope } => {
                let c = Arc::clone(controller);
                let w = Arc::clone(wake_hub);
                executor.spawn(async move {
                    let _ = c.load_earlier(&scope).await;
                    w.wake();
                });
            }
            Intent::Reconnect => {
                let r = Arc::clone(runtime);
                let w = Arc::clone(wake_hub);
                executor.spawn(async move {
                    let _ = r.reconnect().await;
                    w.wake();
                });
            }
            Intent::Select(which) => {
                let target = if which == "b" {
                    scopes[1].clone()
                } else {
                    scopes[0].clone()
                };
                let c = Arc::clone(controller);
                let w = Arc::clone(wake_hub);
                executor.spawn(async move {
                    let _ = c.open(target.clone()).await;
                    let _ = c.select(ComposerDestination::run(target));
                    w.wake();
                });
            }
        }
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if tekes_ui_app_host::run_component_worker_from_args(&args)
        .expect("AppHost component worker dispatch")
    {
        return;
    }

    let offscreen = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--offscreen").then(|| pair[1].clone()));

    let executor = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));
    let (runtime, controller) = executor
        .block_on(tekes_agent_ui_demo::fixture::setup())
        .expect("fixture baseline");
    let scopes = [
        RunScope::main("fixture", "a"),
        RunScope::main("fixture", "b"),
    ];
    let wake_hub = AppHostWakeHub::new();

    // Forward RunController updates cleanly to WindowWake without --frame-pump polling.
    {
        let mut rx = controller.subscribe();
        let hub = Arc::clone(&wake_hub);
        executor.spawn(async move {
            while rx.changed().await.is_ok() {
                hub.wake();
            }
        });
    }

    let mut surface = RunSurface::new();
    let initial = controller.snapshot();
    surface.sync(&initial).expect("initial sync");
    surface.follow_latest();
    let last_revision = initial.revision;
    let surface_rc = Rc::new(RefCell::new(surface));
    let active = Rc::new(Cell::new(true));

    let source = RunSurfaceContainer {
        surface: Rc::clone(&surface_rc),
        controller: Arc::clone(&controller),
        wake_hub: Arc::clone(&wake_hub),
        last_revision,
        active,
    };

    let c_act = Arc::clone(&controller);
    let r_act = Arc::clone(&runtime);
    let e_act = Arc::clone(&executor);
    let s_act = scopes;
    let w_act = wake_hub;
    let surface_act = surface_rc;

    let container = LocalContainer::from_source("agent-run", source)
        .title("AgentKit · AppHost")
        .on_action(move |_host, records: Vec<ContentActionRecord>| {
            use tekes_agent_components_ui::{
                CANCEL, EARLIER, LATEST, RECONNECT, SEND, SESSION_A, SESSION_B,
            };
            let mut intents = Vec::new();
            let mut surface = surface_act.borrow_mut();
            let snap = c_act.snapshot();
            for rec in records {
                if let Some(sel) = snap.selection.as_ref() {
                    if rec.action.target == SEND && !surface.composing() {
                        intents.push(Intent::Send {
                            destination: sel.destination.clone(),
                        });
                    } else if rec.action.target == CANCEL
                        && let ComposerDestination::Run { scope } = &sel.destination
                    {
                        intents.push(Intent::Cancel {
                            scope: scope.clone(),
                        });
                    } else if rec.action.target == EARLIER
                        && let ComposerDestination::Run { scope } = &sel.destination
                    {
                        intents.push(Intent::Earlier {
                            scope: scope.clone(),
                        });
                    }
                }
                if rec.action.target == RECONNECT {
                    intents.push(Intent::Reconnect);
                } else if rec.action.target == SESSION_A {
                    intents.push(Intent::Select("a"));
                } else if rec.action.target == SESSION_B {
                    intents.push(Intent::Select("b"));
                } else if rec.action.target == LATEST {
                    surface.follow_latest();
                }
            }
            if !surface.composing() {
                let text = surface.editor().text().to_owned();
                if text != snap.draft.text
                    && let Some(sel) = snap.selection.as_ref()
                {
                    intents.push(Intent::Edit {
                        destination: sel.destination.clone(),
                        text,
                    });
                }
            }
            drop(surface);
            dispatch_intents(intents, &c_act, &r_act, &e_act, &s_act, &w_act);
        });

    let app = AppWindow::from_containers(
        vec![container.into()],
        PresentationOptions::new("agent-run"),
    )
    .expect("valid AppWindow")
    .theme(Theme::LIGHT);

    if let Some(path) = offscreen {
        let (_menus, mut hosted) = app.into_window_handler().expect("headless AppWindow");
        let mut gpu = Renderer::new_headless().expect("offscreen Metal renderer");
        let rgba = hosted
            .render_headless_rgba8(&mut gpu, (1000, 760), 1)
            .expect("render offscreen PNG");
        let file = std::fs::File::create(&path).expect("create offscreen PNG");
        let mut encoder = png::Encoder::new(file, 1000, 760);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("PNG header")
            .write_image_data(&rgba)
            .expect("PNG bytes");
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), runtime.stop())
                .await
                .ok();
        });
        return;
    }

    app.run_with_args(&[args[0].clone()])
        .expect("AppWindow run");
}
