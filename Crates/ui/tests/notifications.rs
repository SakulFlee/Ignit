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
fn hidden_notifications_do_not_age_until_visible() {
    let mut notifications = UiNotifications::default();
    notifications.max_visible = 1;
    notifications.post(UiNotification::info("First", "Visible").duration(2.0));
    notifications.post(UiNotification::info("Second", "Hidden").duration(2.0));

    // Newest is painted first; the older card waits underneath.
    notifications.advance(1.9);
    assert_eq!(notifications.visible().next().unwrap().title, "Second");
    notifications.advance(0.2);
    assert_eq!(notifications.visible().next().unwrap().title, "First");
    assert_eq!(notifications.len(), 1);

    // The older card only starts aging once it was promoted.
    notifications.advance(1.9);
    assert_eq!(notifications.visible().count(), 1);
    notifications.advance(0.2);
    assert!(notifications.is_empty());
}

#[test]
fn hidden_cards_pause_their_visible_timer() {
    let mut notifications = UiNotifications::default();
    notifications.max_visible = 1;
    notifications.post(UiNotification::info("First", "Paused").duration(2.0));
    notifications.advance(1.0);

    // A newer card takes the only slot; the first card pauses at 1.0s.
    notifications.post(UiNotification::info("Second", "Visible").duration(2.0));
    notifications.advance(1.5);
    notifications.advance(1.0);
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications.visible().next().unwrap().title, "First");

    // Four seconds have passed, but only 1.5 were spent visible.
    notifications.advance(0.5);
    assert_eq!(notifications.visible().count(), 1);
    notifications.advance(0.6);
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
    notifications.post(UiNotification::critical("Offline", "No connection").sticky());
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
        .post(UiNotification::critical("Light toast", "Still readable"));
    update(&mut world, &mut measure);

    let text = text_commands(&world);
    let (_, color) = text
        .iter()
        .find(|(text, _)| text == "Light toast")
        .expect("toast text");
    assert!(color[0] < 0.2, "toast color was {color:?}");
}

#[test]
fn top_placement_is_centered() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    {
        let mut notifications = world.get_resource_mut::<UiNotifications>().unwrap();
        notifications.placement = UiNotificationPlacement::Top;
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
fn placement_supports_cardinal_and_intercardinal_anchors() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::info("Anchored", "Card"));

    let cases = [
        UiNotificationPlacement::TopLeft,
        UiNotificationPlacement::Top,
        UiNotificationPlacement::TopRight,
        UiNotificationPlacement::Right,
        UiNotificationPlacement::BottomRight,
        UiNotificationPlacement::Bottom,
        UiNotificationPlacement::BottomLeft,
        UiNotificationPlacement::Left,
    ];
    for placement in cases {
        if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
            notifications.placement = placement;
        }
        update(&mut world, &mut measure);
        let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
        let rect = draw
            .commands
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Shadow { rect, .. } if (rect.width - 340.0).abs() < 0.5 => {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("toast shadow");
        let x = match placement {
            UiNotificationPlacement::TopLeft
            | UiNotificationPlacement::BottomLeft
            | UiNotificationPlacement::Left => 16.0,
            UiNotificationPlacement::Top | UiNotificationPlacement::Bottom => 230.0,
            UiNotificationPlacement::TopRight
            | UiNotificationPlacement::BottomRight
            | UiNotificationPlacement::Right => 444.0,
        };
        let y = match placement {
            UiNotificationPlacement::TopLeft
            | UiNotificationPlacement::Top
            | UiNotificationPlacement::TopRight => 16.0,
            UiNotificationPlacement::BottomLeft
            | UiNotificationPlacement::Bottom
            | UiNotificationPlacement::BottomRight => 600.0 - 16.0 - rect.height,
            UiNotificationPlacement::Left | UiNotificationPlacement::Right => {
                (600.0 - rect.height) * 0.5
            }
        };
        assert!((rect.x - x).abs() < 0.5, "{placement:?} x was {}", rect.x);
        assert!((rect.y - y).abs() < 0.5, "{placement:?} y was {}", rect.y);
    }
}

#[test]
fn notification_header_shows_severity_label_and_color() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::warning("Heads up", "Body"));
    update(&mut world, &mut measure);

    let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
    let header = draw
        .commands
        .iter()
        .find_map(|command| match command {
            UiDrawCommand::Rect { rect, color, .. }
                if (rect.width - 340.0).abs() < 0.5 && (rect.height - 24.0).abs() < 0.5 =>
            {
                Some(*color)
            }
            _ => None,
        })
        .expect("header bar");
    assert!(header[0] > 0.8 && header[1] > 0.6, "header {header:?}");
    let label = draw
        .commands
        .iter()
        .find_map(|command| match command {
            UiDrawCommand::Text { text, style, .. } if text == "WARNING" => Some(style.color),
            _ => None,
        })
        .expect("severity label");
    // Warning is bright, so the header text must be dark.
    assert!(label[0] < 0.2, "label color {label:?}");
}

#[test]
fn notifications_slide_and_fade_in() {
    let mut world = World::new();
    setup(&mut world);
    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::info("Heads up", "Body"));
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .advance(0.01);
    update(&mut world, &mut measure);

    let background = |world: &World| {
        let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
        draw.commands
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Rect { rect, color, .. }
                    if (rect.width - 340.0).abs() < 0.5 && rect.height > 24.5 =>
                {
                    Some((*rect, *color))
                }
                _ => None,
            })
            .expect("card background")
    };
    let (start, color) = background(&world);
    assert!(start.x > 444.0, "start x was {}", start.x);
    assert!(color[3] < 0.5, "start alpha was {}", color[3]);

    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .advance(0.5);
    update(&mut world, &mut measure);
    let (settled, color) = background(&world);
    assert!(
        (settled.x - 444.0).abs() < 0.5,
        "settled x was {}",
        settled.x
    );
    assert!(color[3] > 0.99, "settled alpha was {}", color[3]);
}

#[test]
fn toasts_anchor_to_the_primary_viewport() {
    let mut world = World::new();
    setup(&mut world);
    // The runtime resizes every root to the window size, including offset
    // overlay roots, so the anchor must not be chosen by area or order.
    let mut overlay = UiDocumentNode::new("offset_overlay");
    overlay.view = Some(UiView {
        size: ignit_ui::UiSize::new(800.0, 600.0),
        offset: UiPoint::new(600.0, 400.0),
        order: 20,
        ..UiView::default()
    });
    UiBuilder::new().root(overlay).build(&mut world).unwrap();

    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    world
        .get_resource_mut::<UiNotifications>()
        .unwrap()
        .post(UiNotification::info("Anchored", "Primary root"));
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

#[test]
fn showcase_roots_receive_a_toast() {
    let mut world = World::new();
    init(&mut world);
    let mut root = UiDocumentNode::new("demo_root")
        .layout(
            UiLayout::column()
                .size(1280.0, 720.0)
                .padding(24.0)
                .gap(12.0),
        )
        .child(UiDocumentNode::new("title").text("Ignit UI widgets"));
    root.view = Some(UiView::new(1280.0, 720.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let ron = include_str!("../../../Examples/ui/Assets/Scenes/ui_demo.ron");
    let document = ignit_ui::UiDocument::from_ron(ron).expect("demo document");
    ignit_ui::spawn_document(&mut world, &document, Some("ui_demo")).unwrap();
    let ron_root = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&"ron_root".into())
        .unwrap();
    if let Some(views) = world.get_component_store_mut::<UiView>()
        && let Some(view) = views.get_mut_store().get_component_mut(ron_root.index)
    {
        // The runtime resizes every root view to the window, even offset
        // overlay roots. Mirror that here.
        view.size = ignit_ui::UiSize::new(1280.0, 720.0);
    }

    let mut measure = ignit_ui::UiFallbackTextMeasure;
    update(&mut world, &mut measure);
    if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
        notifications.placement = UiNotificationPlacement::Top;
        notifications.post(UiNotification::info("Demo notice #1", "Toast body").duration(6.0));
    }
    update(&mut world, &mut measure);

    let draw = world.get_resource::<ignit_ui::UiDrawList>().unwrap();
    let texts: Vec<&str> = draw
        .commands
        .iter()
        .filter_map(|command| match command {
            UiDrawCommand::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Demo notice #1"), "texts: {texts:?}");
    let shadow = draw
        .commands
        .iter()
        .find_map(|command| match command {
            UiDrawCommand::Shadow { rect, .. } if (rect.width - 340.0).abs() < 0.5 => Some(*rect),
            _ => None,
        })
        .expect("toast shadow");
    assert!((shadow.x - 470.0).abs() < 0.5, "toast x was {}", shadow.x);
}
