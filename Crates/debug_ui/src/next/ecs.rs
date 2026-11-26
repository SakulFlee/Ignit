//! New-UI ECS panel.
//!
//! Read-only for now: it lists entities under a filter and shows the selected
//! entity's component type names. Component editing stays in the iced panel
//! until the new-UI editable fields land.

use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiAction, UiColor, UiDimension, UiDocumentNode, UiEdges, UiEvent, UiFocusState, UiLayout,
    UiStyle, UiTextStyle, UiTextWrap, UiWidget, UiWidgetSpec,
};

use crate::state::{DebugMessage, DebugUiState, PanelId};

const WINDOW_ID: &str = "debug_ecs_window";
const FILTER_ID: &str = "debug_ecs_filter";
const STATUS_ID: &str = "debug_ecs_status";
const SELECTED_ID: &str = "debug_ecs_selected";
const DESPAWN_ID: &str = "debug_ecs_despawn";
const ROW_PREFIX: &str = "debug_ecs_entity_";
const COMPONENT_PREFIX: &str = "debug_ecs_component_";

/// Fixed number of entity rows built once and reused by refreshing their text.
const ENTITY_ROWS: usize = 48;
/// Fixed number of component rows for the selected entity.
const COMPONENT_ROWS: usize = 20;
/// How many entities the filter may inspect per refresh.
///
/// A world with hundreds of thousands of entities would otherwise stall the
/// UI thread formatting labels and walking component stores every refresh.
const MAX_SCANNED_ENTITIES: usize = 2_000;

pub(super) fn build() -> UiDocumentNode {
    let left = UiDocumentNode::new("debug_ecs_left")
        .layout(
            UiLayout::column()
                .width(300.0)
                .height_full()
                .gap(4.0)
                .shrink(0.0),
        )
        .child(filter_row())
        .child(
            UiDocumentNode::new(STATUS_ID)
                .layout(UiLayout::default().width_full().height(20.0).shrink(0.0))
                .text("0 entities")
                .style(small_style()),
        )
        .child(entity_list());

    let right = UiDocumentNode::new("debug_ecs_right")
        .layout(UiLayout {
            flex_grow: 1.0,
            ..UiLayout::column().height_full().gap(4.0)
        })
        .child(selected_section())
        .child(component_list());

    let content = UiDocumentNode::new("debug_ecs_body")
        .layout(
            UiLayout::row()
                .absolute()
                .inset(UiEdges {
                    left: UiDimension::Length(10.0),
                    right: UiDimension::Length(10.0),
                    top: UiDimension::Length(34.0),
                    bottom: UiDimension::Length(10.0),
                })
                .gap(8.0),
        )
        .child(left)
        .child(right);

    UiDocumentNode::new(WINDOW_ID)
        .layout(
            UiLayout::column()
                .width(720.0)
                .height(450.0)
                .absolute()
                .inset(UiEdges {
                    left: UiDimension::Length(40.0),
                    right: UiDimension::Auto,
                    top: UiDimension::Length(250.0),
                    bottom: UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "ECS".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel"))
        .child(content)
        .child(super::close_button("debug_ecs_close", "debug_ecs_hide"))
}

fn filter_row() -> UiDocumentNode {
    UiDocumentNode::new("debug_ecs_filter_row")
        .layout(
            UiLayout::row()
                .width_full()
                .height(24.0)
                .gap(4.0)
                .shrink(0.0),
        )
        .child(
            UiDocumentNode::new(FILTER_ID)
                .layout(UiLayout {
                    flex_grow: 1.0,
                    height: UiDimension::Length(24.0),
                    ..Default::default()
                })
                .widget(UiWidgetSpec::TextBox {
                    value: String::new(),
                    placeholder: "filter (ID, label or component)".into(),
                    multiline: false,
                })
                .focusable()
                .style(UiStyle::default().class("panel").padding_text(UiTextStyle {
                    size: 11.0,
                    wrap: UiTextWrap::NoWrap,
                    ..Default::default()
                })),
        )
        .child(
            UiDocumentNode::new("debug_ecs_filter_clear")
                .layout(
                    UiLayout::row()
                        .size(48.0, 24.0)
                        .justify(ignit_ui::UiJustify::Center)
                        .align_items(ignit_ui::UiAlign::Center),
                )
                .widget(UiWidgetSpec::Button {
                    label: "Clear".into(),
                })
                .action(UiAction::new("debug_ecs_filter_clear"))
                .focusable()
                .style(UiStyle::default().class("panel"))
                .child(UiDocumentNode::new("debug_ecs_filter_clear_label").text("Clear")),
        )
}

fn entity_list() -> UiDocumentNode {
    let mut list = UiDocumentNode::new("debug_ecs_entity_scroll").layout(UiLayout {
        flex_grow: 1.0,
        min_height: UiDimension::Length(0.0),
        ..UiLayout::column().width_full().gap(2.0)
    });
    list = list.scroll();

    for index in 0..ENTITY_ROWS {
        list = list.child(
            UiDocumentNode::new(format!("{ROW_PREFIX}{index}"))
                .layout(UiLayout::default().width_full().height(20.0).shrink(0.0))
                .widget(UiWidgetSpec::Button {
                    label: String::new(),
                })
                .action(UiAction::new(format!("{ROW_PREFIX}{index}")))
                .focusable()
                .style(row_style(false, false)),
        );
    }

    list
}

fn selected_section() -> UiDocumentNode {
    UiDocumentNode::new("debug_ecs_selected_row")
        .layout(
            UiLayout::row()
                .width_full()
                .height(26.0)
                .gap(4.0)
                .align_items(ignit_ui::UiAlign::Center)
                .shrink(0.0),
        )
        .child(
            UiDocumentNode::new(SELECTED_ID)
                .layout(UiLayout {
                    flex_grow: 1.0,
                    ..Default::default()
                })
                .text("No entity selected")
                .style(small_style()),
        )
        .child(
            UiDocumentNode::new(DESPAWN_ID)
                .layout(
                    UiLayout::row()
                        .size(72.0, 20.0)
                        .justify(ignit_ui::UiJustify::Center)
                        .align_items(ignit_ui::UiAlign::Center),
                )
                .widget(UiWidgetSpec::Button {
                    label: "Despawn".into(),
                })
                .action(UiAction::new(DESPAWN_ID))
                .focusable()
                .style(UiStyle::default().class("panel").visible(false))
                .child(UiDocumentNode::new("debug_ecs_despawn_label").text("Despawn")),
        )
        .child(
            UiDocumentNode::new("debug_ecs_hide_button").layout(
                UiLayout::row()
                    .size(52.0, 20.0)
                    .justify(ignit_ui::UiJustify::Center)
                    .align_items(ignit_ui::UiAlign::Center),
            ),
        )
}

fn component_list() -> UiDocumentNode {
    let mut list = UiDocumentNode::new("debug_ecs_component_scroll").layout(UiLayout {
        flex_grow: 1.0,
        min_height: UiDimension::Length(0.0),
        ..UiLayout::column().width_full().gap(1.0)
    });
    list = list.scroll();

    for index in 0..COMPONENT_ROWS {
        list = list.child(
            UiDocumentNode::new(format!("{COMPONENT_PREFIX}{index}"))
                .layout(UiLayout::default().width_full().height(18.0).shrink(0.0))
                .style(component_style(false)),
        );
    }

    list
}

fn row_style(visible: bool, selected: bool) -> UiStyle {
    let style = UiStyle::default()
        .radius(4.0)
        .padding_text(UiTextStyle {
            size: 11.0,
            color: if selected {
                UiColor::token("accent")
            } else {
                UiColor::token("text")
            },
            wrap: UiTextWrap::NoWrap,
            ..Default::default()
        })
        .visible(visible);

    if selected {
        style.background(UiColor::Rgba([0.16, 0.32, 0.52, 0.55]))
    } else {
        style
    }
}

fn component_style(visible: bool) -> UiStyle {
    UiStyle::default()
        .padding_text(UiTextStyle {
            size: 11.0,
            color: UiColor::token("text_muted"),
            wrap: UiTextWrap::NoWrap,
            ..Default::default()
        })
        .visible(visible)
}

fn small_style() -> UiStyle {
    UiStyle::default().padding_text(UiTextStyle {
        size: 11.0,
        color: UiColor::token("text_muted"),
        wrap: UiTextWrap::NoWrap,
        ..Default::default()
    })
}

pub(super) fn handle_event(world: &mut World, event: &UiEvent) -> bool {
    match event {
        UiEvent::ValueChanged { id, value, .. } if id.as_str() == FILTER_ID => {
            crate::handle_debug_message_world(DebugMessage::SetFilter(value.clone()), world);
            true
        }
        UiEvent::Action { action, .. } => {
            let name = action.name.as_str();
            if name == "debug_ecs_hide" {
                crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Ecs), world);
                return true;
            }
            if name == "debug_ecs_filter_clear" {
                crate::handle_debug_message_world(DebugMessage::SetFilter(String::new()), world);
                return true;
            }
            if name == DESPAWN_ID {
                let selected = world
                    .get_resource::<DebugUiState>()
                    .and_then(|state| state.selected);
                if let Some(entity) = selected {
                    crate::handle_debug_message_world(DebugMessage::DespawnEntity(entity), world);
                }
                return true;
            }
            // Row actions are a fixed `debug_ecs_entity_{row}` name; the row is
            // mapped back to an entity by replaying the same filter the refresh
            // used. Nothing can change between the frame that drew the row and
            // the click that follows it.
            if let Some(index) = name
                .strip_prefix(ROW_PREFIX)
                .and_then(|value| value.parse::<usize>().ok())
            {
                let filter = world
                    .get_resource::<DebugUiState>()
                    .map(|state| state.filter.clone())
                    .unwrap_or_default();
                let entities = matching_entities(world, &filter);
                if let Some(entity) = entities.get(index).copied() {
                    let already_selected = world
                        .get_resource::<DebugUiState>()
                        .is_some_and(|state| state.selected == Some(entity));
                    let target = if already_selected { None } else { Some(entity) };
                    crate::handle_debug_message_world(DebugMessage::SelectEntity(target), world);
                }
                return true;
            }
            false
        }
        _ => false,
    }
}

pub(super) fn refresh(world: &mut World) {
    let (filter, selected, open) = world.get_resource::<DebugUiState>().map_or_else(
        || (String::new(), None, false),
        |state| {
            (
                state.filter.clone(),
                state.selected,
                state.is_panel_visible(PanelId::Ecs),
            )
        },
    );

    let entities = matching_entities(world, &filter);
    let total = world.entity_count();
    let scanned = total.min(MAX_SCANNED_ENTITIES);

    for index in 0..ENTITY_ROWS {
        let id = format!("{ROW_PREFIX}{index}");
        let Some(row) = find(world, &id) else {
            continue;
        };
        match entities.get(index).copied() {
            Some(entity) => {
                let label = crate::panels::entity_label(world, &entity)
                    .unwrap_or_else(|| "Entity".to_string());
                let label = format!("#{}  {label}", entity.index);
                let text = if selected == Some(entity) {
                    format!("* {label}")
                } else {
                    label
                };
                super::set_text(world, &id, text);
                set_row_style(world, row, true, selected == Some(entity));
            }
            None => set_row_style(world, row, false, false),
        }
    }

    let selected_valid = selected.filter(|entity| world.is_valid(entity));
    let selected_label = selected_valid.map_or_else(
        || "No entity selected".to_string(),
        |entity| {
            let label =
                crate::panels::entity_label(world, &entity).unwrap_or_else(|| "Entity".to_string());
            format!("#{}  {label}", entity.index)
        },
    );
    super::set_text(world, SELECTED_ID, selected_label);

    let components = selected_valid
        .map(|entity| world.entity_components(&entity))
        .unwrap_or_default();

    for index in 0..COMPONENT_ROWS {
        let id = format!("{COMPONENT_PREFIX}{index}");
        let Some(row) = find(world, &id) else {
            continue;
        };
        match components.get(index) {
            Some((_, name)) => {
                super::set_text(world, &id, crate::panels::edit::short_type_name(name));
                set_component_style(world, row, true);
            }
            None => set_component_style(world, row, false),
        }
    }

    let matched = entities.len();
    let shown = matched.min(ENTITY_ROWS);
    let mut status = format!("{shown} of {matched} matching | {total} entities");
    if total > scanned {
        status.push_str(" | scan capped");
    }
    super::set_text(world, STATUS_ID, status);

    set_despawn_visible(world, selected_valid.is_some());
    sync_filter(world, &filter);

    if let Some(window) = find(world, WINDOW_ID) {
        super::set_panel_open(world, window, open);
    }
}

/// Entities matching the current filter, in stable index order.
///
/// Index and generation alone identify a row; the label and component names
/// are only consulted for matching, and are recomputed on the next refresh.
fn matching_entities(world: &World, filter: &str) -> Vec<Entity> {
    let mut entities = world
        .entities()
        .take(MAX_SCANNED_ENTITIES)
        .collect::<Vec<_>>();
    entities.sort_by_key(|entity| entity.index);

    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return entities;
    }

    entities.retain(|entity| entity_matches(world, entity, &needle));
    entities
}

fn entity_matches(world: &World, entity: &Entity, needle: &str) -> bool {
    let index_needle = needle.trim_start_matches('#');
    if !index_needle.is_empty() && entity.index.to_string().contains(index_needle) {
        return true;
    }

    if crate::panels::entity_label(world, entity)
        .is_some_and(|label| label.to_lowercase().contains(needle))
    {
        return true;
    }

    world
        .entity_components(entity)
        .iter()
        .any(|(_, name)| name.to_lowercase().contains(needle))
}

/// Mirrors the state's filter into the text field unless the user is typing.
fn sync_filter(world: &mut World, filter: &str) {
    let Some(entity) = find(world, FILTER_ID) else {
        return;
    };
    if world
        .get_resource::<UiFocusState>()
        .is_some_and(|focus| focus.is_focused(entity))
    {
        return;
    }

    let current = world.get_component_store::<UiWidget>().and_then(|store| {
        store
            .get_component(entity.index)
            .and_then(|widget| match &widget.spec {
                UiWidgetSpec::TextBox { value, .. } => Some(value.clone()),
                _ => None,
            })
    });
    if current.as_deref() == Some(filter) {
        return;
    }

    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::TextBox { value, .. } = &mut widget.spec
    {
        *value = filter.to_string();
        let cursor = value.len();
        widget.cursor = cursor;
        widget.anchor = cursor;
        changed = true;
    }
    if changed {
        super::set_text(world, FILTER_ID, filter.to_string());
        super::mark_dirty(world, entity, ignit_ui::UiDirty::PAINT);
    }
}

fn set_row_style(world: &mut World, entity: Entity, visible: bool, selected: bool) {
    set_style(world, entity, row_style(visible, selected));
}

fn set_component_style(world: &mut World, entity: Entity, visible: bool) {
    set_style(world, entity, component_style(visible));
}

fn set_style(world: &mut World, entity: Entity, style: UiStyle) {
    let current = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned());
    if current.as_ref() == Some(&style) {
        return;
    }
    let _ = world.attach_component(&entity, style);
    super::mark_dirty(
        world,
        entity,
        ignit_ui::UiDirty::LAYOUT | ignit_ui::UiDirty::PAINT,
    );
}

fn set_despawn_visible(world: &mut World, visible: bool) {
    let Some(entity) = find(world, DESPAWN_ID) else {
        return;
    };
    let current = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned())
        .unwrap_or_default();
    if current.visible == Some(visible) {
        return;
    }
    let style = current.visible(visible);
    let _ = world.attach_component(&entity, style);
    super::mark_dirty(
        world,
        entity,
        ignit_ui::UiDirty::LAYOUT | ignit_ui::UiDirty::PAINT,
    );
}

fn find(world: &World, id: &str) -> Option<Entity> {
    world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
}

#[cfg(test)]
mod tests {
    use ignit_ecs_bridge::CameraDescriptor;
    use ignit_ui::UiId;
    use winit::keyboard::KeyCode;

    use super::*;
    use crate::next::build_document;

    fn test_world() -> World {
        let mut world = World::new();
        world.insert_resource(DebugUiState::new(KeyCode::F1));
        world
    }

    fn send(world: &mut World, event: UiEvent) {
        let mut events = world
            .get_resource_mut::<ignit_ecs::Events<UiEvent>>()
            .expect("ui events resource");
        events.send(event);
    }

    fn text_box_value(world: &World, entity: Entity) -> Option<String> {
        world.get_component_store::<UiWidget>().and_then(|store| {
            store
                .get_component(entity.index)
                .and_then(|widget| match &widget.spec {
                    UiWidgetSpec::TextBox { value, .. } => Some(value.clone()),
                    _ => None,
                })
        })
    }

    #[test]
    fn document_builds_with_the_ecs_window_and_fixed_rows() {
        let mut world = test_world();
        let (root, _) = build_document(&mut world).expect("document");

        let forest = world.get_resource::<ignit_ui::UiForest>().unwrap();
        assert!(forest.find(&WINDOW_ID.into()).is_some());
        assert!(forest.find(&FILTER_ID.into()).is_some());
        assert!(forest.find(&"debug_ecs_entity_0".into()).is_some());
        assert!(
            forest
                .find(&format!("debug_ecs_entity_{}", ENTITY_ROWS - 1).into())
                .is_some()
        );
        assert!(
            forest
                .find(&format!("debug_ecs_component_{}", COMPONENT_ROWS - 1).into())
                .is_some()
        );
        assert!(forest.find(&"debug_ecs_despawn".into()).is_some());
        assert!(forest.find(&crate::next::ROOT_ID.into()).is_some());
        assert_eq!(forest.find(&crate::next::ROOT_ID.into()), Some(root));
    }

    #[test]
    fn filter_event_updates_the_state() {
        let mut world = test_world();
        let (_, panel) = build_document(&mut world).expect("document");
        let filter = world
            .get_resource::<ignit_ui::UiForest>()
            .unwrap()
            .find(&FILTER_ID.into())
            .expect("filter entity");

        send(
            &mut world,
            UiEvent::ValueChanged {
                entity: filter,
                id: UiId::from(FILTER_ID),
                value: "camera".into(),
            },
        );
        crate::next::handle_events(&mut world, panel);

        assert_eq!(
            world.get_resource::<DebugUiState>().unwrap().filter,
            "camera"
        );
    }

    #[test]
    fn row_action_selects_the_matching_entity() {
        let mut world = test_world();
        let entity = world.spawn_entity();
        world
            .attach_component(&entity, CameraDescriptor::default())
            .expect("camera attach");

        let (_, panel) = build_document(&mut world).expect("document");
        let row = world
            .get_resource::<ignit_ui::UiForest>()
            .unwrap()
            .find(&"debug_ecs_entity_0".into())
            .expect("row entity");

        send(
            &mut world,
            UiEvent::Action {
                entity: row,
                id: UiId::from("debug_ecs_entity_0"),
                action: UiAction::new("debug_ecs_entity_0"),
            },
        );
        crate::next::handle_events(&mut world, panel);

        assert_eq!(
            world.get_resource::<DebugUiState>().unwrap().selected,
            Some(entity)
        );
    }

    #[test]
    fn refresh_populates_the_rows_after_layout() {
        let mut world = test_world();
        let entity = world.spawn_entity();
        world
            .attach_component(&entity, CameraDescriptor::default())
            .expect("camera attach");

        let (root, _) = build_document(&mut world).expect("document");
        world.get_resource_mut::<DebugUiState>().unwrap().visible = true;
        crate::next::set_visible(&mut world, root, true);
        refresh(&mut world);

        let mut measure = ignit_ui::UiFallbackTextMeasure;
        ignit_ui::update(&mut world, &mut measure);

        let window = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&WINDOW_ID.into()))
            .expect("window");
        let computed = world
            .get_component_store::<ignit_ui::UiComputed>()
            .and_then(|store| store.get_component(window.index).copied())
            .expect("window layout");
        assert!(computed.visible);
        assert_eq!((computed.rect.width, computed.rect.height), (600.0, 460.0));

        let row = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_ecs_entity_0".into()))
            .expect("row");
        let text = world
            .get_component_store::<ignit_ui::UiText>()
            .and_then(|store| store.get_component(row.index).cloned())
            .expect("row text");
        assert!(text.value.contains(&format!("#{}", entity.index)));
        assert!(text.value.contains("Camera"));
    }

    #[test]
    fn refresh_mirrors_the_filter_unless_the_field_is_focused() {
        let mut world = test_world();
        let _ = build_document(&mut world).expect("document");
        let filter = world
            .get_resource::<ignit_ui::UiForest>()
            .unwrap()
            .find(&FILTER_ID.into())
            .expect("filter entity");

        world.get_resource_mut::<DebugUiState>().unwrap().filter = "camera".into();
        refresh(&mut world);
        assert_eq!(text_box_value(&world, filter).as_deref(), Some("camera"));

        ignit_ui::focus_entity(&mut world, filter);
        world.get_resource_mut::<DebugUiState>().unwrap().filter = "cam".into();
        refresh(&mut world);
        assert_eq!(
            text_box_value(&world, filter).as_deref(),
            Some("camera"),
            "typing must not be overwritten by the refresh"
        );
    }

    #[test]
    fn filter_matches_index_label_and_component_name() {
        let mut world = test_world();
        let camera = world.spawn_entity();
        world
            .attach_component(&camera, CameraDescriptor::default())
            .expect("camera attach");

        assert!(entity_matches(&world, &camera, &camera.index.to_string()));
        assert!(entity_matches(&world, &camera, "camera"));
        assert!(entity_matches(&world, &camera, "cameradescriptor"));
        assert!(!entity_matches(&world, &camera, "definitely-missing"));
    }
}
