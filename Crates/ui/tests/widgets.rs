use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiBuilder, UiComputed, UiDocumentNode, UiDrawCommand, UiDrawList, UiFocusState, UiInputState,
    UiLayout, UiPoint, UiText, UiTheme, UiThemes, UiView, UiWidget, UiWidgetSpec, init, update,
};

fn build(world: &mut World, id: &str, widget: UiWidgetSpec) -> Entity {
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(400.0, 300.0).padding(20.0).gap(8.0))
        .child(
            UiDocumentNode::new(id)
                .layout(UiLayout::default().size(240.0, 32.0))
                .widget(widget)
                .focusable(),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(world).unwrap();
    world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&id.into())
        .unwrap()
}

fn rect(world: &World, entity: Entity) -> UiComputed {
    *world
        .get_component_store::<UiComputed>()
        .unwrap()
        .get_component(entity.index)
        .unwrap()
}

fn computed(world: &World, id: &str) -> UiComputed {
    let entity = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&id.into())
        .unwrap();
    rect(world, entity)
}

#[test]
fn toggle_and_checkbox_flip_on_click() {
    for widget in [
        UiWidgetSpec::Toggle { on: false },
        UiWidgetSpec::Checkbox { checked: false },
    ] {
        let mut world = World::new();
        init(&mut world);
        let entity = build(&mut world, "control", widget);
        let mut text = ignit_ui::UiFallbackTextMeasure;
        update(&mut world, &mut text);
        let rect = rect(&world, entity);
        let input = UiInputState {
            pointer: UiPoint::new(rect.rect.x + 10.0, rect.rect.y + 10.0),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        world.insert_resource(input.clone());
        update(&mut world, &mut text);
        let mut input = world
            .get_resource::<UiInputState>()
            .map(|input| (*input).clone())
            .unwrap();
        input.primary_down = false;
        input.primary_pressed = false;
        input.primary_released = true;
        world.insert_resource(input);
        update(&mut world, &mut text);
        // A stale click must not replay and toggle the widget back off.
        update(&mut world, &mut text);

        let widget = world
            .get_component_store::<UiWidget>()
            .unwrap()
            .get_component(entity.index)
            .cloned()
            .unwrap();
        match &widget.spec {
            UiWidgetSpec::Toggle { on } => assert!(*on),
            UiWidgetSpec::Checkbox { checked } => assert!(*checked),
            _ => panic!("unexpected widget"),
        }
    }
}

#[test]
fn slider_updates_while_dragging() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "slider",
        UiWidgetSpec::Slider {
            value: 0.0,
            min: 0.0,
            max: 100.0,
        },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let rect = rect(&world, entity);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.rect.x + rect.rect.width * 0.5, rect.rect.y + 10.0),
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.rect.x + rect.rect.width * 0.75, rect.rect.y + 10.0),
        primary_down: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    let UiWidgetSpec::Slider { value, .. } = &widget.spec else {
        panic!("unexpected widget");
    };
    assert!(*value > 70.0);
}

#[test]
fn text_box_edits_when_focused() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "text",
        UiWidgetSpec::TextBox {
            value: String::new(),
            placeholder: "Type".into(),
            multiline: false,
        },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    ignit_ui::focus_entity(&mut world, entity);
    world.insert_resource(UiInputState {
        text: "hello".into(),
        ..Default::default()
    });
    update(&mut world, &mut text);
    let value = world
        .get_component_store::<UiText>()
        .unwrap()
        .get_component(entity.index)
        .unwrap()
        .value
        .clone();
    assert_eq!(value, "hello");
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    let UiWidgetSpec::TextBox { value, .. } = &widget.spec else {
        panic!("unexpected widget");
    };
    assert_eq!(value, "hello");
}

#[test]
fn ron_showcase_document_parses() {
    let source = include_str!("../../../Examples/ui/Assets/Scenes/ui_demo.ron");
    let document = ignit_ui::UiDocument::from_ron(source).expect("showcase RON parses");
    document.validate().expect("showcase RON IDs");
    let mut world = World::new();
    init(&mut world);
    ignit_ui::spawn_document(&mut world, &document, Some("showcase")).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    assert!(
        world
            .get_resource::<ignit_ui::UiForest>()
            .unwrap()
            .find(&"ron_title".into())
            .is_some()
    );
}

#[test]
fn widget_paint_commands_are_appended() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "progress",
        UiWidgetSpec::ProgressBar { value: 0.5 },
    );
    let _ = entity;
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let draw = world.get_resource::<UiDrawList>().unwrap();
    assert!(
        draw.commands
            .iter()
            .any(|command| matches!(command, UiDrawCommand::Rect { .. }))
    );
    assert!(
        world
            .get_resource::<UiFocusState>()
            .unwrap()
            .focused
            .is_none()
    );
}

#[test]
fn text_box_backspace_deletes_a_character() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "text_backspace",
        UiWidgetSpec::TextBox {
            value: "abc".into(),
            placeholder: String::new(),
            multiline: false,
        },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    ignit_ui::focus_entity(&mut world, entity);
    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::Backspace],
        ..Default::default()
    });
    update(&mut world, &mut text);
    let value = world
        .get_component_store::<UiText>()
        .unwrap()
        .get_component(entity.index)
        .unwrap()
        .value
        .clone();
    assert_eq!(value, "ab");
}

#[test]
fn wheel_scrolls_a_scroll_container() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("scroll_root")
        .layout(UiLayout::column().size(400.0, 300.0).padding(10.0))
        .child(
            UiDocumentNode::new("scroll_area")
                .layout(UiLayout::default().width(300.0).height(100.0))
                .scroll()
                .child(
                    UiDocumentNode::new("scroll_content")
                        .layout(UiLayout::column().height(300.0).shrink(0.0)),
                ),
        );
    root.view = Some(UiView::new(400.0, 300.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let scroll_entity = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&"scroll_area".into())
        .unwrap();
    let rect = rect(&world, scroll_entity);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.rect.x + 10.0, rect.rect.y + 10.0),
        wheel: UiPoint::new(0.0, 25.0),
        ..Default::default()
    });
    update(&mut world, &mut text);
    update(&mut world, &mut text);
    let scroll = world
        .get_component_store::<ignit_ui::UiScroll>()
        .unwrap()
        .get_component(scroll_entity.index)
        .copied()
        .unwrap();
    assert!(scroll.offset.y > 0.0);
}

#[test]
fn root_offset_moves_layout_and_hit_testing() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("offset_root")
        .layout(UiLayout::column().size(200.0, 100.0))
        .child(
            UiDocumentNode::new("offset_button")
                .layout(UiLayout::default().size(100.0, 40.0))
                .focusable(),
        );
    root.view = Some(UiView {
        offset: UiPoint::new(300.0, 200.0),
        ..UiView::new(200.0, 100.0)
    });
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let root_rect = computed(&world, "offset_root").rect;
    assert_eq!(root_rect.x, 300.0);
    assert_eq!(root_rect.y, 200.0);
    let button = computed(&world, "offset_button").rect;
    assert_eq!(button.x, 300.0);
    assert_eq!(button.y, 200.0);
    let hit = ignit_ui::hit_test(&world, UiPoint::new(button.x + 5.0, button.y + 5.0)).unwrap();
    assert_eq!(hit.id.as_str(), "offset_button");
    assert!(ignit_ui::hit_test(&world, UiPoint::new(5.0, 5.0)).is_none());
}

#[test]
fn light_theme_draw_text_is_dark() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("light_root")
        .layout(UiLayout::column().size(300.0, 100.0))
        .child(UiDocumentNode::new("light_label").text("Light text"));
    root.view = Some(UiView::new(300.0, 100.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    if let Some(mut themes) = world.get_resource_mut::<UiThemes>() {
        themes.active = UiTheme::light();
    }
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let draw = world.get_resource::<UiDrawList>().unwrap();
    let command = draw
        .commands
        .iter()
        .find(|command| matches!(command, UiDrawCommand::Text { text, .. } if text == "Light text"))
        .unwrap();
    let UiDrawCommand::Text { style, .. } = command else {
        unreachable!()
    };
    assert!(style.color[0] < 0.2, "text color was {:?}", style.color);
}

#[test]
fn switching_theme_updates_text_color() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("switch_root")
        .layout(UiLayout::column().size(300.0, 100.0))
        .child(UiDocumentNode::new("switch_label").text("Switch text"));
    root.view = Some(UiView::new(300.0, 100.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    if let Some(mut themes) = world.get_resource_mut::<UiThemes>() {
        themes.active = UiTheme::light();
    }
    update(&mut world, &mut text);
    let draw = world.get_resource::<UiDrawList>().unwrap();
    let command = draw
        .commands
        .iter()
        .find(
            |command| matches!(command, UiDrawCommand::Text { text, .. } if text == "Switch text"),
        )
        .unwrap();
    let UiDrawCommand::Text { style, .. } = command else {
        unreachable!()
    };
    assert!(style.color[0] < 0.2, "text color was {:?}", style.color);
}
