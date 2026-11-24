use ignit_ecs::World;
use ignit_ui::{
    UiBuilder, UiDocumentNode, UiDrawCommand, UiLayout, UiNotification, UiNotificationPlacement,
    UiNotificationSeverity, UiNotifications, UiPoint, UiTextStyle, UiTheme, UiView, hit_test, init,
    update,
};

fn setup(world: &mut World) {
    init(world);
    let mut root = UiDocumentNode::new("toast_root")
        .layout(UiLayout::column().size(800.0, 600.0).padding(16.0))
        .child(UiDocumentNode::new("toast_content").text("Underlay"));
    root.view = Some(UiView::new(800.0, 600.0));
    UiBuilder::new().root(root).build(world).unwrap();
}

fn text_commands(world: &World) -> Vec<(String, [f32; 4])> {
    world
        .get_resource::<ignit_ui::UiDrawList>()
        .unwrap()
        .commands
        .iter()
        .filter_map(|command| match command {
            UiDrawCommand::Text { text, style, .. } => Some((text.clone(), style.color)),
            _ => None,
        })
        .collect()
}

#[test]
fn notifications_expire_after_their_duration() {
    let mut world = World::new();
    setup(&mut world);
    let mut notifications = UiNotifications::default();
    notifications.post(UiNotification::info("Saved", "Scene stored").duration(2.0));
    assert_eq!(notifications.visible().count(), 1);

    notifications.advance(1.9);
    assert_eq!(notifications.visible().count(), 1);
    notifications.advance(0.2);
    assert_eq!(notifications.visible().count(), 0);
    assert!(notifications.is_empty());
}

#[test]
fn keyed_notifications_coalesce_and_count_repeats() {
    let mut notifications = UiNotifications::default();
    notifications.post(UiNotification::warning("GPU", "Pressure").key("gpu.pressure"));
    notifications.post(UiNotification::warning("GPU", "Pressure").key("gpu.pressure"));

    assert_eq!(notifications.len(), 1);
    let entry = &notifications.entries()[0];
    assert_eq!(entry.repeat, 2);
    assert_eq!(entry.severity, UiNotificationSeverity::Warning);
    assert!(notifications.dismiss("gpu.pressure"));
    assert!(notifications.is_empty());
}

#[test]
fn sticky_notifications_survive_advance() {
    let mut notifications = UiNotifications::default();
    notifications.post(UiNotification::error("Offline", "No connection").sticky());
    notifications.advance(60.0);
    assert_eq!(notifications.visible().count(), 1);
}

#[test]
fn update_paints_toasts_above_the_layout() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::success("Saved", "Scene stored"));
    update(&mut world, &mut measure);

    let commands = &world
        .get_resource::<ignit_ui::UiDrawList>()
        .unwrap()
        .commands;
    let toast = commands
        .iter()
        .position(|command| matches!(command, UiDrawCommand::Text { text, .. } if text == "Saved"))
        .expect("toast title");
    let content = commands
        .iter()
        .position(
            |command| matches!(command, UiDrawCommand::Text { text, .. } if text == "Underlay"),
        )
        .expect("layout text");
    assert!(toast > content);
    assert!(commands.iter().any(
        |command| matches!(command, UiDrawCommand::Text { text, .. } if text == "Scene stored")
    ));
}

#[test]
fn toasts_do_not_add_nodes_or_change_hit_testing() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);

    let point = UiPoint::new(620.0, 60.0);
    let before = hit_test(&world, point).map(|hit| hit.entity);
    let nodes_before = world.entities().count();

    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::info("Heads up", "Nothing to click here"));
    update(&mut world, &mut measure);

    assert_eq!(world.entities().count(), nodes_before);
    assert_eq!(hit_test(&world, point).map(|hit| hit.entity), before);
}

#[test]
fn light_theme_toast_text_uses_dark_tokens() {
    let mut world = World::new();
    setup(&mut world);
    if let Some(mut themes) = world.get_resource_mut::<ignit_ui::UiThemes>() {
        themes.active = UiTheme::light();
    }
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::error("Light toast", "Still readable"));
    update(&mut world, &mut measure);

    let text = text_commands(&world);
    let (_, color) = text
        .iter()
        .find(|(text, _)| text == "Light toast")
        .expect("toast text");
    assert!(color[0] < 0.2, "toast color was {color:?}");
}

#[test]
fn top_center_placement_is_centered() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    {
        let mut notifications = world.get_resource_mut::<UiNotifications>().unwrap();
        notifications.placement = UiNotificationPlacement::TopCenter;
        notifications.width = 300.0;
        notifications.post(UiNotification::info("Centered", "Card"));
    }
    update(&mut world, &mut measure);

    let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
    let shadows: Vec<_> = draw
        .commands
        .iter()
        .filter_map(|command| match command {
            UiDrawCommand::Shadow { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(shadows.len(), 1);
    assert!(
        (shadows[0].x - 250.0).abs() < 0.5,
        "toast x was {}",
        shadows[0].x
    );
}

#[test]
fn toasts_anchor_to_the_largest_viewport() {
    let mut world = World::new();
    setup(&mut world);
    let mut overlay = UiDocumentNode::new("small_overlay");
    overlay.view = Some(UiView {
        size: ignit_ui::UiSize::new(200.0, 100.0),
        order: 20,
        ..UiView::default()
    });
    UiBuilder::new().root(overlay).build(&mut world).unwrap();

    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::info("Anchored", "Largest root"));
    update(&mut world, &mut measure);

    let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
    let shadow = draw
        .commands
        .iter()
        .find_map(|command| match command {
            UiDrawCommand::Shadow { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("toast shadow");
    assert_eq!(shadow.width, 340.0);
    assert!((shadow.x - 444.0).abs() < 0.5, "toast x was {}", shadow.x);
}

#[test]
fn max_visible_limits_the_overlay() {
    let mut notifications = UiNotifications::default();
    notifications.max_visible = 2;
    for index in 0..5 {
        notifications.post(UiNotification::info(format!("Toast {index}"), "Body"));
    }
    assert_eq!(notifications.visible().count(), 2);
}

#[test]
fn default_text_style_keeps_wrapping() {
    // Guards the resolved style used by the toast painter.
    let resolved = ignit_ui::UiResolvedTextStyle::from(&UiTextStyle::default());
    assert_eq!(resolved.wrap, ignit_ui::UiTextWrap::Word);
}
