use ignit_ecs::{Events, World};
use ignit_ui::{
    UiAction, UiBuilder, UiComputed, UiDocument, UiDocumentNode, UiEvent, UiFallbackTextMeasure,
    UiFocusState, UiFocusable, UiInputCapture, UiInputState, UiLayout, UiPoint, UiRect, UiStyle,
    UiText, UiTextStyle, UiTheme, UiView, build_draw_list, focus_next, hit_test, init,
    process_pointer, update,
};

fn computed(world: &World, id: &str) -> UiComputed {
    let entity = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
        .expect("node exists");
    *world
        .get_component_store::<UiComputed>()
        .expect("computed store")
        .get_component(entity.index)
        .expect("computed node")
}

fn node_id(world: &World, id: &str) -> ignit_ecs::Entity {
    world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
        .expect("node exists")
}

#[test]
fn builder_lays_out_nested_panels() {
    let mut world = World::new();
    init(&mut world);
    let root = UiDocumentNode::new("root")
        .layout(
            UiLayout::column()
                .size(800.0, 600.0)
                .padding(20.0)
                .gap(10.0),
        )
        .style(UiStyle::default().background(ignit_ui::UiColor::rgba([0.1, 0.1, 0.1, 1.0])))
        .child(
            UiDocumentNode::new("panel")
                .layout(UiLayout::column().width(300.0).height(200.0).padding(8.0))
                .child(
                    UiDocumentNode::new("label")
                        .layout(UiLayout::default())
                        .text("Hello UI"),
                )
                .child(
                    UiDocumentNode::new("button")
                        .layout(UiLayout::default().size(120.0, 36.0))
                        .focusable()
                        .action(UiAction::new("play")),
                ),
        );
    let mut root = root;
    root.view = Some(UiView::new(800.0, 600.0));
    let handles = UiBuilder::new().root(root).build(&mut world).unwrap();
    assert_eq!(handles.len(), 1);

    let mut measure = UiFallbackTextMeasure;
    let stats = update(&mut world, &mut measure);
    assert_eq!(stats.roots, 1);
    assert!(stats.nodes >= 4);
    assert!(stats.draw_commands >= 2);

    let root_rect = computed(&world, "root").rect;
    assert_eq!(root_rect, UiRect::new(0.0, 0.0, 800.0, 600.0));
    let panel = computed(&world, "panel").rect;
    assert_eq!(panel.x, 20.0);
    assert_eq!(panel.y, 20.0);
    assert_eq!(panel.width, 300.0);
    assert_eq!(panel.height, 200.0);
    let button = computed(&world, "button").rect;
    assert!(button.width > 100.0);
    assert!(button.height > 30.0);
}

#[test]
fn layout_is_skipped_when_nothing_is_dirty() {
    let mut world = World::new();
    init(&mut world);
    let root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0))
        .child(UiDocumentNode::new("label").text("Hello"));
    let mut root = root;
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    let first = update(&mut world, &mut measure);
    assert!(first.layout_ms > 0.0);
    let second = update(&mut world, &mut measure);
    assert_eq!(second.layout_ms, 0.0);
}

#[test]
fn ron_document_round_trips_and_spawns() {
    let document = UiDocument {
        version: 1,
        roots: vec![{
            let mut root = UiDocumentNode::new("hud")
                .layout(UiLayout::row().size(640.0, 360.0).gap(8.0))
                .child(UiDocumentNode::new("title").text("HUD"));
            root.view = Some(UiView::new(640.0, 360.0));
            root
        }],
    };
    let ron = document.to_ron().unwrap();
    let parsed = UiDocument::from_ron(&ron).unwrap();
    parsed.validate().unwrap();
    assert_eq!(parsed.roots[0].id.as_str(), "hud");

    let mut world = World::new();
    init(&mut world);
    ignit_ui::spawn_document(&mut world, &parsed, Some("hud")).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    assert!(computed(&world, "hud").rect.width > 0.0);
}

#[test]
fn pointer_click_emits_events_and_focuses() {
    let mut world = World::new();
    init(&mut world);
    let root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0).padding(20.0))
        .child(
            UiDocumentNode::new("button")
                .layout(UiLayout::default().size(120.0, 36.0))
                .focusable()
                .action(UiAction::new("play")),
        );
    let mut root = root;
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    let button = computed(&world, "button").rect;
    let mut input = UiInputState {
        pointer: UiPoint::new(button.x + 5.0, button.y + 5.0),
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    };
    let mut capture = UiInputCapture::default();
    let hit = process_pointer(&mut world, &mut input, &mut capture);
    assert_eq!(hit.unwrap().id.as_str(), "button");
    assert_eq!(
        world
            .get_resource::<UiFocusState>()
            .unwrap()
            .focused
            .map(|entity| entity.index),
        Some(node_id(&world, "button").index)
    );

    input.primary_down = false;
    input.primary_pressed = false;
    input.primary_released = true;
    process_pointer(&mut world, &mut input, &mut capture);

    let events = world.get_resource::<Events<UiEvent>>().unwrap();
    let events: Vec<&UiEvent> = events.read().collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, UiEvent::Click { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, UiEvent::Action { action, .. } if action.name == "play"))
    );
}

#[test]
fn hit_test_respects_clip_and_visibility() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0))
        .child(
            UiDocumentNode::new("clipped")
                .layout(UiLayout::default().size(100.0, 50.0))
                .child(UiDocumentNode::new("child").layout(UiLayout::default().size(100.0, 100.0))),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    let child = computed(&world, "child");
    assert!(hit_test(&world, UiPoint::new(child.rect.x + 2.0, child.rect.y + 2.0)).is_some());
    // A point outside the root is not hittable.
    assert!(hit_test(&world, UiPoint::new(900.0, 900.0)).is_none());
}

#[test]
fn tab_focus_cycles_focusable_nodes() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0))
        .child(
            UiDocumentNode::new("a")
                .layout(UiLayout::default().size(40.0, 20.0))
                .focusable(),
        )
        .child(
            UiDocumentNode::new("b")
                .layout(UiLayout::default().size(40.0, 20.0))
                .focusable(),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    focus_next(&mut world, false);
    let first = world
        .get_resource::<UiFocusState>()
        .unwrap()
        .focused
        .unwrap();
    focus_next(&mut world, false);
    let second = world
        .get_resource::<UiFocusState>()
        .unwrap()
        .focused
        .unwrap();
    assert_ne!(first, second);
    focus_next(&mut world, true);
    assert_eq!(
        world.get_resource::<UiFocusState>().unwrap().focused,
        Some(first)
    );
    assert_eq!(
        world
            .get_component_store::<UiFocusable>()
            .unwrap()
            .components
            .len(),
        2
    );
}

#[test]
fn theme_color_change_skips_layout() {
    let mut theme = UiTheme::dark();
    let style = UiStyle::default().background(ignit_ui::UiColor::token("accent"));
    let first = theme.resolve_style(&style, &[], ignit_ui::UiState::Normal);
    let layout_revision = theme.layout_revision;
    theme.set_color("accent", [1.0, 0.0, 0.0, 1.0]);
    let second = theme.resolve_style(&style, &[], ignit_ui::UiState::Normal);
    assert_ne!(first.background, second.background);
    assert_eq!(theme.layout_revision, layout_revision);
}

#[test]
fn document_rejects_duplicate_ids() {
    let document = UiDocument {
        version: 1,
        roots: vec![UiDocumentNode::new("same"), UiDocumentNode::new("same")],
    };
    assert!(document.validate().is_err());
}

#[test]
fn draw_list_contains_background_and_text() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0).padding(10.0))
        .style(UiStyle::default().background(ignit_ui::UiColor::rgba([0.1, 0.2, 0.3, 1.0])))
        .child(
            UiDocumentNode::new("label")
                .layout(UiLayout::default())
                .text("hello")
                .style(UiStyle::default().padding_text(UiTextStyle::default())),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    let mut draw = ignit_ui::UiDrawList::default();
    build_draw_list(&world, &mut draw);
    assert!(
        draw.commands
            .iter()
            .any(|command| matches!(command, ignit_ui::UiDrawCommand::Rect { .. }))
    );
    assert!(
        draw.commands
            .iter()
            .any(|command| matches!(command, ignit_ui::UiDrawCommand::Text { .. }))
    );
}

#[test]
fn hot_reload_restores_scroll_text_and_focus_by_id() {
    let mut world = World::new();
    init(&mut world);
    let document = UiDocument {
        version: 1,
        roots: vec![{
            let mut root = UiDocumentNode::new("root")
                .layout(UiLayout::column().size(400.0, 300.0))
                .child(
                    UiDocumentNode::new("scroll")
                        .layout(UiLayout::default().height(100.0))
                        .scroll()
                        .child(
                            UiDocumentNode::new("text")
                                .layout(UiLayout::default().height(200.0).shrink(0.0))
                                .text("old")
                                .focusable(),
                        ),
                );
            root.view = Some(UiView::new(400.0, 300.0));
            root
        }],
    };
    let roots = ignit_ui::spawn_document(&mut world, &document, Some("doc")).unwrap();
    if let Some(mut registry) = world.get_resource_mut::<ignit_ui::UiDocumentRegistry>() {
        registry.entries.insert(
            "doc".into(),
            ignit_ui::UiDocumentEntry {
                path: "test.ron".into(),
                document: document.clone(),
                modified: None,
                roots,
            },
        );
    }
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    let text_entity = node_id(&world, "text");
    let scroll_entity = node_id(&world, "scroll");
    world
        .get_component_store_mut::<UiText>()
        .unwrap()
        .get_mut_store()
        .attach(
            text_entity.index,
            UiText {
                value: "typed".into(),
            },
        );
    world
        .get_component_store_mut::<ignit_ui::UiScroll>()
        .unwrap()
        .get_mut_store()
        .attach(
            scroll_entity.index,
            ignit_ui::UiScroll {
                offset: UiPoint::new(0.0, 42.0),
                ..Default::default()
            },
        );
    ignit_ui::focus_entity(&mut world, text_entity);

    let mut next = document.clone();
    next.roots[0].children[0].layout = UiLayout::default().height(140.0);
    next.roots[0].children[0].children[0].text = Some(UiText::new("new"));
    let source = next.to_ron().unwrap();
    ignit_ui::reload_document_from_source(&mut world, "doc", &source).unwrap();
    update(&mut world, &mut measure);

    let text_entity = node_id(&world, "text");
    assert_eq!(
        world
            .get_component_store::<UiText>()
            .unwrap()
            .get_component(text_entity.index)
            .unwrap()
            .value,
        "typed"
    );
    assert_eq!(
        world
            .get_component_store::<ignit_ui::UiScroll>()
            .unwrap()
            .get_component(node_id(&world, "scroll").index)
            .unwrap()
            .offset
            .y,
        42.0
    );
    assert_eq!(
        world
            .get_resource::<UiFocusState>()
            .unwrap()
            .focused
            .map(|entity| entity.index),
        Some(text_entity.index)
    );
}

#[test]
fn scroll_container_clips_hit_testing() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0))
        .child(
            UiDocumentNode::new("scroll")
                .layout(UiLayout::default().width(200.0).height(50.0))
                .scroll()
                .child(
                    UiDocumentNode::new("child")
                        .layout(UiLayout::default().width(200.0).height(200.0)),
                ),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    let scroll = computed(&world, "scroll");
    let child = computed(&world, "child");
    assert!(child.clip.is_some());
    let outside = UiPoint::new(child.rect.x + 2.0, scroll.rect.y + scroll.rect.height + 5.0);
    let hit = hit_test(&world, outside);
    assert!(hit.is_none_or(|hit| hit.id.as_str() != "child"));
}

#[test]
fn style_classes_apply_state_overrides() {
    let mut theme = UiTheme::dark();
    theme.set_class(
        "button",
        ignit_ui::UiStyleClass {
            style: ignit_ui::UiStylePatch {
                background: Some(ignit_ui::UiColor::rgba([0.2, 0.2, 0.2, 1.0])),
                ..Default::default()
            },
            states: ignit_ui::UiStateStyles {
                hovered: Some(ignit_ui::UiStylePatch {
                    background: Some(ignit_ui::UiColor::rgba([0.4, 0.4, 0.4, 1.0])),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );
    let style = UiStyle::default().class("button");
    let normal = theme.resolve_style(&style, &[], ignit_ui::UiState::Normal);
    let hovered = theme.resolve_style(&style, &[], ignit_ui::UiState::Hovered);
    assert_eq!(normal.background, Some([0.2, 0.2, 0.2, 1.0]));
    assert_eq!(hovered.background, Some([0.4, 0.4, 0.4, 1.0]));
}

#[test]
fn text_component_and_dirty_reset() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(200.0, 100.0))
        .child(UiDocumentNode::new("t").text("a"));
    root.view = Some(UiView::new(200.0, 100.0));
    let handles = UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut measure = UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    let label = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&"t".into())
        .unwrap();
    assert_eq!(
        world
            .get_component_store::<UiText>()
            .unwrap()
            .get_component(label.index)
            .unwrap()
            .value,
        "a"
    );
    handles[0].set_text(&mut world, "changed");
    update(&mut world, &mut measure);
    let dirty = world
        .get_component_store::<ignit_ui::UiDirty>()
        .unwrap()
        .get_component(label.index)
        .copied()
        .unwrap();
    assert!(!dirty.contains(ignit_ui::UiDirty::LAYOUT));
}

#[test]
fn light_theme_resolves_dark_text() {
    let theme = UiTheme::light();
    let resolved = theme.resolve_style(&UiStyle::default(), &[], ignit_ui::UiState::Normal);
    let color = resolved.text.expect("body text").color;
    assert!(color[0] < 0.2 && color[1] < 0.2 && color[2] < 0.2);
}
