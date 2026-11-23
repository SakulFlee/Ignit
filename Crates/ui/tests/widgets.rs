use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiBuilder, UiComputed, UiDocumentNode, UiDrawCommand, UiDrawList, UiFocusState, UiInputState,
    UiLayout, UiPoint, UiText, UiView, UiWidget, UiWidgetSpec, init, update,
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
