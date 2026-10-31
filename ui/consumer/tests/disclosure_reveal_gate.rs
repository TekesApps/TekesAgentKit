//! Public-SDK geometry gate, prepared but NOT_RUN. No pixel/native acceptance.
//! The proxy models the proposed delegation only to expose its clip contract;
//! production must not claim passthrough unless these positive cases pass.
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
#[path = "support/disclosure_host.rs"]
mod disclosure_host;
use disclosure_host::render;

#[derive(Debug)]
struct Proxy {
    disclosure: DisclosureContent,
    arranged: bool,
}
impl Component for Proxy {
    fn children(&self) -> &[Content] {
        self.disclosure.children()
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.disclosure.children_mut()
    }
    fn validate(&self) -> Result<(), CompositionError> {
        self.disclosure.validate()
    }
    fn lifecycle(&mut self, event: Lifecycle) {
        self.arranged = false;
        self.disclosure.lifecycle(event);
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        self.arranged = false;
        self.disclosure.measure(text, size)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.disclosure.arrange(text, frame, clip);
        self.arranged = true;
    }
    fn paints_children(&self) -> bool {
        true
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.disclosure.paint(scene, text, frame, clip, theme);
    }
    fn host_reveal_passthrough(&self) -> bool {
        self.arranged && self.disclosure.host_reveal_passthrough()
    }
    fn host_reveal_child_vertical_clip(
        &self,
        child: ComponentId,
    ) -> Option<HostRevealVerticalClip> {
        self.disclosure.host_reveal_child_vertical_clip(child)
    }
}
fn body() -> Content {
    Content::new(
        ComponentId(4),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![Content::new(ComponentId(5), Button::new("original leaf"))],
        },
    )
}
#[test]
fn original_stack_is_a_positive_reveal_control() {
    let mut host = ContentHost::new(body()).unwrap();
    render(&mut host);
    host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default())
        .expect("original Stack supports reveal");
}
#[test]
fn delegated_open_body_must_preserve_the_original_reveal_contract() {
    let mut host = ContentHost::new(Content::new(
        ComponentId(1),
        Proxy {
            disclosure: DisclosureContent::new(
                ComponentId(2),
                Content::new(ComponentId(3), TextContent::new("folder")),
                body(),
                true,
            ),
            arranged: false,
        },
    ))
    .unwrap();
    render(&mut host);
    let result = host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default());
    assert!(
        result.is_ok(),
        "AC5 positive gate: delegated original body reveal rejected: {result:?}"
    );
}
#[test]
fn closed_body_must_not_become_revealable_through_the_proxy() {
    let mut host = ContentHost::new(Content::new(
        ComponentId(1),
        Proxy {
            disclosure: DisclosureContent::new(
                ComponentId(2),
                Content::new(ComponentId(3), TextContent::new("folder")),
                body(),
                false,
            ),
            arranged: false,
        },
    ))
    .unwrap();
    render(&mut host);
    assert!(matches!(
        host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default()),
        Err(HostRevealError::UnavailableTarget)
    ));
}

const SCROLL: ComponentId = ComponentId(10);
const LEAF: ComponentId = ComponentId(5);
#[derive(Debug)]
struct FixedHeight(f32);
impl Component for FixedHeight {
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(available.width, self.0))
    }
}
fn offscreen_body() -> Content {
    Content::new(
        ComponentId(4),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![
                Content::new(ComponentId(6), FixedHeight(900.)),
                Content::new(LEAF, Button::new("original offscreen leaf")),
            ],
        },
    )
}
fn scroll_host(content: Content) -> ContentHost {
    ContentHost::new(Content::new(
        SCROLL,
        ScrollContent::new(tekes_ui_controls::ScrollArea::default(), content),
    ))
    .unwrap()
}
fn offscreen_disclosure(expanded: bool) -> ContentHost {
    scroll_host(Content::new(
        ComponentId(1),
        Proxy {
            disclosure: DisclosureContent::new(
                ComponentId(2),
                Content::new(ComponentId(3), TextContent::new("folder")),
                offscreen_body(),
                expanded,
            ),
            arranged: false,
        },
    ))
}
fn dump_original_geometry(stage: &str, host: &ContentHost) {
    let scroll = host.get(SCROLL).unwrap();
    let model = &scroll.component::<ScrollContent>().unwrap().scroll;
    eprintln!(
        "AC5_STAGE {stage} scroll_id={:?} scroll_mount={} viewport_frame={:?} viewport_size={:?} actual_offset={:?}",
        scroll.id(),
        scroll.mount_identity(),
        scroll.frame(),
        model.viewport_size(),
        model.offset
    );
    for id in [ComponentId(1), ComponentId(2), ComponentId(4), LEAF] {
        let node = host.get(id).unwrap();
        eprintln!(
            "AC5_STAGE {stage} node_id={:?} mount={} frame={:?} visible={:?}",
            id,
            node.mount_identity(),
            node.frame(),
            node.visible()
        );
    }
    if let Some(proxy) = host.get(ComponentId(1)).unwrap().component::<Proxy>() {
        eprintln!(
            "AC5_STAGE {stage} sdk_declared_body_layout={:?}",
            proxy.disclosure.body_layout()
        );
    }
}
fn assert_nonzero_original_scroll_reveal(stage: &str, host: &mut ContentHost) {
    dump_original_geometry(&format!("{stage}:before-plan"), host);
    let original_leaf = host.get(LEAF).unwrap().mount_identity();
    let original_scroll = host.get(SCROLL).unwrap().mount_identity();
    assert_eq!(
        host.get(SCROLL)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y,
        0.
    );
    assert!(
        host.get(LEAF).unwrap().frame().min_y() > 300.,
        "the fixture's original target must stay offscreen"
    );
    assert!(host.get(LEAF).unwrap().visible().is_none());
    let plan_result = host.plan_host_reveal(
        LEAF,
        tekes_ui_controls::ScrollRevealOptions {
            animated: false,
            ..Default::default()
        },
        &[],
        Default::default(),
    );
    eprintln!(
        "AC5_STAGE {stage}:plan_result={:?}",
        plan_result.as_ref().map(|_| ())
    );
    let plan =
        plan_result.expect("offscreen original leaf requires a real supported Scroll projection");
    let outcome = host
        .commit_host_reveal(plan)
        .expect("original Scroll commit");
    assert!(
        outcome.changed,
        "fully-visible fastpath cannot satisfy this gate"
    );
    assert!(
        host.get(SCROLL)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y
            > 0.,
        "must change the actual original Scroll offset"
    );
    render(host);
    dump_original_geometry(&format!("{stage}:after-commit-and-render"), host);
    let actual_leaf = host.get(LEAF).unwrap().frame();
    assert_eq!(
        host.get(LEAF).unwrap().visible(),
        Some(actual_leaf),
        "actual re-arranged original leaf must land wholly in the original viewport"
    );
    assert_eq!(host.get(LEAF).unwrap().id(), LEAF);
    assert_eq!(host.get(SCROLL).unwrap().id(), SCROLL);
    assert_eq!(host.get(LEAF).unwrap().mount_identity(), original_leaf);
    assert_eq!(host.get(SCROLL).unwrap().mount_identity(), original_scroll);
    eprintln!("AC5_STAGE {stage}:NONZERO_OFFSET_AND_ACTUAL_LANDING_PASSED");
}
fn matched_offscreen_hosts() -> (ContentHost, ContentHost) {
    let mut disclosure = offscreen_disclosure(true);
    render(&mut disclosure);
    let header = disclosure.get(ComponentId(2)).unwrap().frame();
    let body = disclosure.get(ComponentId(4)).unwrap().frame();
    let original_leaf_frame = disclosure.get(LEAF).unwrap().frame();
    // Match the SDK's already arranged header/clearance in the original Stack
    // control. Neither fixture moves or replaces the offscreen target.
    let mut stack = scroll_host(Content::new(
        ComponentId(1),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: body.min_y() - header.max_y(),
            },
            flexible_child: None,
            children: vec![
                Content::new(ComponentId(2), FixedHeight(header.size.height)),
                offscreen_body(),
            ],
        },
    ));
    render(&mut stack);
    assert_eq!(
        stack.get(LEAF).unwrap().frame(),
        original_leaf_frame,
        "same original Scroll and target geometry"
    );
    (stack, disclosure)
}
#[test]
fn original_stack_offscreen_scroll_must_commit_and_land() {
    let (mut stack, _) = matched_offscreen_hosts();
    assert_nonzero_original_scroll_reveal("independent-stack-control", &mut stack);
}
#[test]
fn delegated_offscreen_body_matches_the_original_stack_scroll_contract() {
    let (mut stack, mut disclosure) = matched_offscreen_hosts();
    assert_nonzero_original_scroll_reveal("paired-stack-control", &mut stack);
    assert_nonzero_original_scroll_reveal("paired-disclosure", &mut disclosure);
}
#[test]
fn the_same_scroll_rejects_the_closed_offscreen_disclosure_body() {
    let mut host = offscreen_disclosure(false);
    render(&mut host);
    let original_leaf = host.get(LEAF).unwrap().mount_identity();
    let original_scroll = host.get(SCROLL).unwrap().mount_identity();
    assert!(matches!(
        host.plan_host_reveal(
            LEAF,
            tekes_ui_controls::ScrollRevealOptions {
                animated: false,
                ..Default::default()
            },
            &[],
            Default::default()
        ),
        Err(HostRevealError::UnavailableTarget)
    ));
    assert_eq!(
        host.get(SCROLL)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y,
        0.
    );
    assert_eq!(host.get(LEAF).unwrap().mount_identity(), original_leaf);
    assert_eq!(host.get(SCROLL).unwrap().mount_identity(), original_scroll);
}
