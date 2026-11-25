//! # Ignit UI core
//!
//! ECS-native retained UI: documents, Taffy layout, themes, focus, input
//! routing, and a renderer-agnostic draw list. The text and renderer crates
//! consume [`UiDrawList`] and implement [`UiTextMeasure`].
//!
//! The core does not know about wgpu or winit. `ignit_app` owns the UI phase
//! and feeds [`UiInputState`]; `ignit_ui_render` records the draw list into the
//! main scene encoder.

pub mod builder;
pub mod document;
pub mod draw;
pub mod events;
pub mod focus;
pub mod forest;
pub mod geometry;
pub mod id;
pub mod input;
pub mod layout;
pub mod layout_engine;
pub mod node;
pub mod notifications;
pub mod style;
pub mod style_pass;
pub mod widgets;

pub use builder::{UiBuilder, UiHandle};
pub use document::{
    UI_DOCUMENT_VERSION, UiDocument, UiDocumentEntry, UiDocumentNode, UiDocumentRegistry,
    load_document, reload_changed_documents, reload_document, reload_document_from_source,
    spawn_document,
};
pub use draw::{UiDrawCommand, UiDrawList, UiStats, build_draw_list};
pub use events::UiEvent;
pub use focus::{UiDirection, UiFocusState, blur, focus_direction, focus_entity, focus_next};
pub use forest::{UiForest, UiIndex};
pub use geometry::{UiPoint, UiRect, UiSize};
pub use id::UiId;
pub use input::{
    UiHit, UiInputCapture, UiInputState, UiKey, UiModifiers, hit_test, process_pointer,
};
pub use layout::{
    UiAlign, UiDimension, UiDisplay, UiEdges, UiFlexDirection, UiFlexWrap, UiGap, UiGridAutoFlow,
    UiGridLine, UiGridPlacement, UiJustify, UiLayout, UiLengthPercentage, UiOverflow, UiPosition,
    UiTrack,
};
pub use layout_engine::{
    UiFallbackTextMeasure, UiLayoutEngine, UiLayoutState, UiReconcileState, UiTextMeasure,
};
pub use node::{
    UiAction, UiActionKind, UiComputed, UiDirty, UiFocusable, UiInteraction, UiNode, UiRoot,
    UiScroll, UiScrollContainer, UiThemeScope, UiView,
};
pub use notifications::{
    UiNotification, UiNotificationPlacement, UiNotificationSeverity, UiNotifications,
    append_notification_commands,
};
pub use style::{
    UiBorder, UiColor, UiCorners, UiCursorIcon, UiImage, UiImageStyle, UiResolvedBorder,
    UiResolvedImage, UiResolvedShadow, UiResolvedStyle, UiResolvedTextStyle, UiRgba, UiShadow,
    UiState, UiStateStyles, UiStyle, UiStyleClass, UiStylePatch, UiText, UiTextStyle, UiTextWrap,
    UiTheme, UiThemes,
};
pub use widgets::{UiWidget, UiWidgetSpec, append_widget_commands, update_widgets};

use std::time::Instant;

use ignit_ecs::{Events, World};

/// Ensures the UI resources exist. Safe to call more than once.
pub fn init(world: &mut World) {
    if world.get_resource::<UiForest>().is_none() {
        world.insert_resource(UiForest::default());
    }
    if world.get_resource::<UiThemes>().is_none() {
        world.insert_resource(UiThemes::with_active(UiTheme::dark()));
    }
    if world.get_resource::<UiInputState>().is_none() {
        world.insert_resource(UiInputState::default());
    }
    if world.get_resource::<UiInputCapture>().is_none() {
        world.insert_resource(UiInputCapture::default());
    }
    if world.get_resource::<UiFocusState>().is_none() {
        world.insert_resource(UiFocusState::default());
    }
    if world.get_resource::<Events<UiEvent>>().is_none() {
        world.insert_resource(Events::<UiEvent>::default());
    }
    if world
        .get_resource::<std::sync::Arc<UiLayoutState>>()
        .is_none()
    {
        world.insert_resource(std::sync::Arc::new(UiLayoutState::default()));
    }
    if world.get_resource::<UiDrawList>().is_none() {
        world.insert_resource(UiDrawList::default());
    }
    if world.get_resource::<UiNotifications>().is_none() {
        world.insert_resource(UiNotifications::default());
    }
    if world.get_resource::<UiStats>().is_none() {
        world.insert_resource(UiStats::default());
    }
    if world.get_resource::<UiReconcileState>().is_none() {
        world.insert_resource(UiReconcileState::default());
    }
    if world.get_resource::<UiDocumentRegistry>().is_none() {
        world.insert_resource(UiDocumentRegistry::default());
    }
}

/// Runs one UI frame: resolve styles, layout if needed, route input, update
/// focus, and rebuild the draw list.
///
/// The runtime calls this before the game schedule so game systems see the
/// same-frame [`UiInputCapture`] and [`UiEvent`] queue.
pub fn update(world: &mut World, measure: &mut dyn UiTextMeasure) -> UiStats {
    init(world);
    // UI events live for exactly one frame. Clear the previous frame before
    // hit-testing, then game systems read the events emitted by this frame.
    if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
        events.clear();
    }
    let start = Instant::now();

    let style_start = Instant::now();
    let style_nodes = style_pass::resolve_styles(world);
    let style_ms = style_start.elapsed().as_secs_f32() * 1000.0;

    let layout_start = Instant::now();
    let layout_state = world
        .get_resource::<std::sync::Arc<UiLayoutState>>()
        .map(|state| std::sync::Arc::clone(&state));
    let layout_ran = layout_state
        .map(|layout| layout.compute(world, measure))
        .unwrap_or(false);
    let layout_ms = layout_start.elapsed().as_secs_f32() * 1000.0;

    let mut input = world
        .get_resource::<UiInputState>()
        .map(|input| (*input).clone())
        .unwrap_or_default();
    let mut capture = UiInputCapture::default();
    let hit = process_pointer(world, &mut input, &mut capture);

    let focused = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);

    if input.key_pressed(UiKey::Tab) {
        focus::focus_next(world, input.modifiers.shift);
        capture.keyboard = true;
    }
    for (key, direction) in [
        (UiKey::ArrowLeft, UiDirection::Left),
        (UiKey::ArrowRight, UiDirection::Right),
        (UiKey::ArrowUp, UiDirection::Up),
        (UiKey::ArrowDown, UiDirection::Down),
    ] {
        if input.key_pressed(key) {
            focus::focus_direction(world, direction);
            capture.keyboard = true;
        }
    }
    if input.key_pressed(UiKey::Escape) {
        focus::blur(world);
        capture.keyboard = true;
    }
    if input.key_pressed(UiKey::Enter) {
        if let Some(entity) = focused
            && let Some(id) = world.get_component_store::<UiNode>().and_then(|store| {
                store
                    .get_component(entity.index)
                    .map(|node| node.id.clone())
            })
        {
            let value = world
                .get_component_store::<UiText>()
                .and_then(|store| {
                    store
                        .get_component(entity.index)
                        .map(|text| text.value.clone())
                })
                .unwrap_or_default();
            if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
                events.send(UiEvent::Submit { entity, id, value });
            }
        }
        capture.keyboard = true;
    }

    if let Some(entity) = focused {
        capture.keyboard = true;
        if world
            .get_component_store::<UiText>()
            .is_some_and(|store| store.get_component(entity.index).is_some())
        {
            capture.text = true;
        }
    }
    if hit.is_some() {
        capture.pointer = true;
    }

    widgets::update_widgets(world, &input);
    input.end_frame();
    world.insert_resource(input);
    world.insert_resource(capture);

    let draw_start = Instant::now();
    let mut draw = world
        .get_resource::<UiDrawList>()
        .map(|list| list.clone())
        .unwrap_or_default();
    draw::build_draw_list(world, &mut draw);
    widgets::append_widget_commands(world, &mut draw);
    draw.sort_by_z(world);
    // Screen-space toasts paint above every root, so they are appended after
    // the z sort instead of taking part in it.
    notifications::append_notification_commands(world, measure, &mut draw);
    let draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;
    let commands = draw.command_count();
    world.insert_resource(draw);

    let stats = UiStats {
        nodes: world
            .get_component_store::<UiNode>()
            .map(|store| store.components.len())
            .unwrap_or(0),
        roots: world
            .get_resource::<UiForest>()
            .map(|forest| forest.roots().len())
            .unwrap_or(0),
        draw_commands: commands,
        layout_ms: if layout_ran { layout_ms } else { 0.0 },
        style_ms,
    };
    world.insert_resource(stats.clone());
    let _ = (style_nodes, draw_ms, start);
    stats
}
