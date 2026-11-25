use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiBuilder, UiClipboardResource, UiComputed, UiDocumentNode, UiDrawCommand, UiDrawList,
    UiFocusState, UiInputState, UiLayout, UiPoint, UiSize, UiText, UiTextMeasure, UiTextStyle,
    UiTheme, UiThemes, UiView, UiWidget, UiWidgetSpec, init, update,
};

struct TenPixelMeasure;

impl UiTextMeasure for TenPixelMeasure {
    fn measure(&mut self, text: &str, style: &UiTextStyle, max_width: Option<f32>) -> UiSize {
        ignit_ui::UiFallbackTextMeasure.measure(text, style, max_width)
    }

    fn cursor_index(
        &mut self,
        text: &str,
        _style: &UiTextStyle,
        _max_width: f32,
        x: f32,
        _y: f32,
    ) -> usize {
        let index = ((x / 10.0).round().max(0.0) as usize).min(text.chars().count());
        text.char_indices()
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or(text.len())
    }
}

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
fn text_box_arrows_home_end_and_word_selection() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "text_keys",
        UiWidgetSpec::TextBox {
            value: "hello world".into(),
            placeholder: String::new(),
            multiline: false,
        },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    ignit_ui::focus_entity(&mut world, entity);

    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::ArrowLeft, ignit_ui::UiKey::ArrowLeft],
        ..Default::default()
    });
    update(&mut world, &mut text);
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!(widget.cursor, 9);
    assert_eq!(
        world.get_resource::<UiFocusState>().unwrap().focused,
        Some(entity)
    );

    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::Home],
        ..Default::default()
    });
    update(&mut world, &mut text);
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!(widget.cursor, 0);

    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::End],
        ..Default::default()
    });
    update(&mut world, &mut text);
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!(widget.cursor, 11);

    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::ArrowLeft],
        modifiers: ignit_ui::UiModifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        ..Default::default()
    });
    update(&mut world, &mut text);
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!((widget.anchor, widget.cursor), (11, 6));
}

#[test]
fn text_box_drag_beyond_the_edge_scrolls() {
    let mut world = World::new();
    init(&mut world);
    let value = "abcdefghij".repeat(10);
    let entity = build(
        &mut world,
        "text_scroll_select",
        UiWidgetSpec::TextBox {
            value: value.clone(),
            placeholder: String::new(),
            multiline: false,
        },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    ignit_ui::focus_entity(&mut world, entity);
    let rect = rect(&world, entity);
    let y = rect.content.y + 10.0;

    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.content.x + 231.0, y),
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    let pressed_anchor = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .unwrap()
        .anchor;
    assert!(pressed_anchor > 0);
    for _ in 0..40 {
        world.insert_resource(UiInputState {
            pointer: UiPoint::new(rect.content.x + 450.0, y),
            primary_down: true,
            ..Default::default()
        });
        update(&mut world, &mut text);
    }
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert!(widget.scroll_x > 0.0, "scroll was {}", widget.scroll_x);
    assert_eq!(widget.cursor, value.len());
    assert_eq!(widget.anchor, pressed_anchor);

    for _ in 0..40 {
        world.insert_resource(UiInputState {
            pointer: UiPoint::new(rect.content.x - 200.0, y),
            primary_down: true,
            ..Default::default()
        });
        update(&mut world, &mut text);
    }
    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!(widget.scroll_x, 0.0);
    assert_eq!(widget.cursor, 0);
}

#[test]
fn text_box_mouse_selection_copy_and_paste() {
    let mut world = World::new();
    init(&mut world);
    let entity = build(
        &mut world,
        "text_selection",
        UiWidgetSpec::TextBox {
            value: "hello world".into(),
            placeholder: String::new(),
            multiline: false,
        },
    );
    let mut text = TenPixelMeasure;
    update(&mut world, &mut text);
    let rect = rect(&world, entity);
    let y = rect.content.y + 10.0;

    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.content.x + 60.0, y),
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.content.x + 10.0, y),
        primary_down: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(rect.content.x + 10.0, y),
        primary_released: true,
        ..Default::default()
    });
    update(&mut world, &mut text);

    let widget = world
        .get_component_store::<UiWidget>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!((widget.anchor, widget.cursor), (6, 1));
    {
        let draw = world.get_resource::<UiDrawList>().unwrap();
        assert!(draw.commands.iter().any(|command| matches!(
            command,
            UiDrawCommand::Text {
                selection: Some((1, 6)),
                ..
            }
        )));
    }

    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::Character('c')],
        modifiers: ignit_ui::UiModifiers {
            ctrl: true,
            ..Default::default()
        },
        ..Default::default()
    });
    update(&mut world, &mut text);
    {
        let clipboard = world.get_resource::<UiClipboardResource>().unwrap();
        assert_eq!(clipboard.get_text().as_deref(), Some("ello "));
    }

    world
        .get_resource::<UiClipboardResource>()
        .unwrap()
        .set_text("XYZ".into());
    world.insert_resource(UiInputState {
        keys_pressed: vec![ignit_ui::UiKey::Character('v')],
        modifiers: ignit_ui::UiModifiers {
            ctrl: true,
            ..Default::default()
        },
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
    assert_eq!(value, "hXYZworld");
}

#[test]
fn clicking_a_floating_window_raises_it() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("raise_root").layout(UiLayout::column().size(700.0, 500.0));
    for (id, left, top) in [("lower", 10.0, 10.0), ("upper", 120.0, 120.0)] {
        root = root.child(
            UiDocumentNode::new(id)
                .layout(
                    UiLayout::column()
                        .width(300.0)
                        .height(200.0)
                        .absolute()
                        .inset(ignit_ui::UiEdges {
                            left: ignit_ui::UiDimension::Length(left),
                            right: ignit_ui::UiDimension::Auto,
                            top: ignit_ui::UiDimension::Length(top),
                            bottom: ignit_ui::UiDimension::Auto,
                        }),
                )
                .widget(UiWidgetSpec::FloatingWindow {
                    title: id.into(),
                    open: true,
                }),
        );
    }
    root.view = Some(UiView::new(700.0, 500.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);

    let lower = computed(&world, "lower");
    let upper = computed(&world, "upper");
    assert!(upper.z_index > lower.z_index);

    world.insert_resource(UiInputState {
        pointer: UiPoint::new(lower.rect.x + 20.0, lower.rect.y + 10.0),
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    update(&mut world, &mut text);

    let lower = computed(&world, "lower");
    let upper = computed(&world, "upper");
    assert!(
        lower.z_index > upper.z_index,
        "lower {} vs upper {}",
        lower.z_index,
        upper.z_index
    );
}

#[test]
fn floating_window_resizes_from_the_grip() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("float_root")
        .layout(UiLayout::column().size(600.0, 400.0).padding(20.0))
        .child(
            UiDocumentNode::new("float_window")
                .layout(
                    UiLayout::column()
                        .width(300.0)
                        .height(200.0)
                        .absolute()
                        .inset(ignit_ui::UiEdges {
                            left: ignit_ui::UiDimension::Length(10.0),
                            right: ignit_ui::UiDimension::Auto,
                            top: ignit_ui::UiDimension::Length(10.0),
                            bottom: ignit_ui::UiDimension::Auto,
                        }),
                )
                .widget(UiWidgetSpec::FloatingWindow {
                    title: "Float".into(),
                    open: true,
                }),
        );
    root.view = Some(UiView::new(600.0, 400.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let rect = computed(&world, "float_window");
    let grip = UiPoint::new(
        rect.rect.x + rect.rect.width - 5.0,
        rect.rect.y + rect.rect.height - 5.0,
    );

    world.insert_resource(UiInputState {
        pointer: grip,
        primary_down: true,
        primary_pressed: true,
        ..Default::default()
    });
    update(&mut world, &mut text);
    world.insert_resource(UiInputState {
        pointer: UiPoint::new(grip.x + 50.0, grip.y + 30.0),
        primary_down: true,
        ..Default::default()
    });
    update(&mut world, &mut text);

    let entity = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&"float_window".into())
        .unwrap();
    let layout = world
        .get_component_store::<UiLayout>()
        .unwrap()
        .get_component(entity.index)
        .cloned()
        .unwrap();
    assert_eq!(layout.width, ignit_ui::UiDimension::Length(350.0));
    assert_eq!(layout.height, ignit_ui::UiDimension::Length(230.0));
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

#[test]
fn golden_progress_draw_list_signature() {
    let mut world = World::new();
    init(&mut world);
    let _entity = build(
        &mut world,
        "progress_golden",
        UiWidgetSpec::ProgressBar { value: 0.5 },
    );
    let mut text = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut text);
    let draw = world.get_resource::<UiDrawList>().unwrap();
    let signature: Vec<&str> = draw
        .commands
        .iter()
        .map(|command| match command {
            UiDrawCommand::Shadow { .. } => "shadow",
            UiDrawCommand::Rect { .. } => "rect",
            UiDrawCommand::Border { .. } => "border",
            UiDrawCommand::Text { .. } => "text",
            UiDrawCommand::Image { .. } => "image",
            UiDrawCommand::Line { .. } => "line",
        })
        .collect();
    assert_eq!(signature, vec!["rect", "rect"]);
}
