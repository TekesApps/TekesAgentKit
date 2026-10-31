use super::*;

#[test]
fn launcher_uses_source_three_by_two_slots_and_keeps_hidden_slots_at_library_scales() {
    for theme in Theme::ALL {
        for scale in [1., 1.4, 2.] {
            for width in [220., 400., 720.] {
                let mut text = Text::new(scale, true);
                let frame = Rect::new(20., 30., width, 400.);
                let row = Button::new("M").measure(&mut text, frame.size).size.height;
                let mut input = SidePanelLauncherInput {
                    revision: 1,
                    owner: SidePanelLauncherOwner::Workspace {
                        id: "public-consumer".into(),
                    },
                    destinations: SidePanelDestination::ALL
                        .into_iter()
                        .map(|destination| SidePanelDestinationItem {
                            destination,
                            title: destination.title().into(),
                            available: true,
                            enabled: true,
                        })
                        .collect(),
                };
                let mut launcher = SidePanelLauncher::new(63200, "Side Panel Views");
                launcher.update(&input).unwrap();
                let root = launcher.id();
                let ids: Vec<_> = SidePanelDestination::ALL
                    .into_iter()
                    .map(|destination| launcher.button_id(destination).unwrap())
                    .collect();
                let mut host = launcher.into_host().unwrap();
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let rects: Vec<_> = ids
                    .iter()
                    .map(|id| host.get(*id).unwrap().frame())
                    .collect();
                let gap = tekes_ui_tokens::Spacing::Relaxed.value();
                let close = |a: f32, b: f32| assert!((a - b).abs() < 0.02, "{a} != {b}");
                for rect in &rects {
                    close(rect.size.width, rects[0].size.width);
                    close(rect.size.height, rects[0].size.height);
                    assert!(rect.size.width <= row * 4. + 0.02);
                    assert!(rect.size.height <= row * 3. + 0.02);
                }
                close(
                    rects[0].min_x() + rects[2].max_x(),
                    frame.min_x() + frame.max_x(),
                );
                close(
                    rects[0].min_y() + rects[5].max_y(),
                    frame.min_y() + frame.max_y(),
                );
                close(rects[1].min_x() - rects[0].max_x(), gap);
                close(rects[3].min_y() - rects[0].max_y(), gap);
                close(rects[0].min_x(), rects[3].min_x());
                let original =
                    host.get(ids[0]).unwrap().component::<Button>().unwrap() as *const _ as usize;
                input.revision = 2;
                for index in 1..5 {
                    input.destinations[index].available = false;
                }
                host.get_mut(root)
                    .unwrap()
                    .component_mut::<SidePanelLauncher>()
                    .unwrap()
                    .update(&input)
                    .unwrap();
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                assert_eq!(host.get(ids[0]).unwrap().frame(), rects[0]);
                assert_eq!(host.get(ids[5]).unwrap().frame(), rects[5]);
                assert_eq!(
                    host.get(ids[0]).unwrap().component::<Button>().unwrap() as *const _ as usize,
                    original
                );
                assert!(ids[1..5].iter().all(|id| host.get(*id).is_none()));
            }
        }
    }
}
