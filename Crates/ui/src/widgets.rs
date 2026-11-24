//! v1 widget state machines and paint commands.
//!
//! Widgets are ordinary ECS nodes with a [`UiWidget`] component. Interaction
//! is driven by the shared [`UiInputState`] and [`UiEvent`] stream, so widgets
//! never store closures.

use std::collections::HashSet;

use ignit_ecs::{Entity, Events, World};
use serde::{Deserialize, Serialize};

use crate::draw::{UiDrawCommand, UiDrawList};
use crate::events::UiEvent;
use crate::focus::{self, UiFocusState};
use crate::forest::UiForest;
use crate::geometry::UiRect;
use crate::id::UiId;
use crate::input::UiInputState;
use crate::layout::{UiDimension, UiLayout, UiPosition};
use crate::node::{UiComputed, UiDirty};
use crate::style::{UiResolvedStyle, UiResolvedTextStyle, UiStyle, UiText, UiTextWrap};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UiWidgetSpec {
    #[default]
    None,
    Button {
        label: String,
    },
    Toggle {
        on: bool,
    },
    Checkbox {
        checked: bool,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
    },
    TextBox {
        value: String,
        placeholder: String,
        multiline: bool,
    },
    ProgressBar {
        value: f32,
    },
    Dropdown {
        options: Vec<String>,
        selected: usize,
        open: bool,
    },
    ListBox {
        items: Vec<String>,
        selected: usize,
    },
    Tabs {
        tabs: Vec<String>,
        active: usize,
        #[serde(default)]
        contents: Vec<UiId>,
    },
    Collapsible {
        open: bool,
        content: Option<UiId>,
    },
    FloatingWindow {
        title: String,
        open: bool,
    },
    Sparkline {
        values: Vec<f32>,
        max: f32,
    },
    Tooltip {
        text: String,
    },
    Popup {
        open: bool,
    },
    Menu {
        open: bool,
    },
    Modal {
        open: bool,
    },
    Divider,
}

#[derive(Clone, Debug)]
pub struct UiWidget {
    pub spec: UiWidgetSpec,
    pub dragging: bool,
    pub drag_start: Option<(f32, f32)>,
    pub drag_base: (f32, f32),
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    pub hovered_option: Option<usize>,
}

impl UiWidget {
    pub fn new(spec: UiWidgetSpec) -> Self {
        let cursor = match &spec {
            UiWidgetSpec::TextBox { value, .. } => value.len(),
            _ => 0,
        };
        Self {
            spec,
            dragging: false,
            drag_start: None,
            drag_base: (0.0, 0.0),
            cursor,
            anchor: cursor,
            scroll_x: 0.0,
            hovered_option: None,
        }
    }
}

/// Runs widget state changes. Call after hit testing/focus and before the draw
/// list is built.
pub fn update_widgets(world: &mut World, input: &UiInputState) {
    let events: Vec<UiEvent> = world
        .get_resource::<Events<UiEvent>>()
        .map(|events| events.read().cloned().collect())
        .unwrap_or_default();
    let clicks: HashSet<UiId> = events
        .iter()
        .filter_map(|event| match event {
            UiEvent::Click { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let focused = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);

    let entities: Vec<Entity> = world.entities().collect();
    let mut changes: Vec<(Entity, UiWidget, Option<UiEvent>)> = Vec::new();
    let mut text_changes: Vec<(Entity, String)> = Vec::new();

    for entity in entities {
        let Some(mut widget) = world
            .get_component_store::<UiWidget>()
            .and_then(|store| store.get_component(entity.index).cloned())
        else {
            continue;
        };
        let Some(rect) = world
            .get_component_store::<UiComputed>()
            .and_then(|store| store.get_component(entity.index).copied())
        else {
            continue;
        };
        let id = world
            .get_component_store::<crate::node::UiNode>()
            .and_then(|store| {
                store
                    .get_component(entity.index)
                    .map(|node| node.id.clone())
            })
            .unwrap_or_default();

        let mut spec = widget.spec.clone();
        let mut event = None;
        match &mut spec {
            UiWidgetSpec::Toggle { on } if clicks.contains(&id) => {
                *on = !*on;
                event = Some(UiEvent::ValueChanged {
                    entity,
                    id: id.clone(),
                    value: on.to_string(),
                });
            }
            UiWidgetSpec::Checkbox { checked } if clicks.contains(&id) => {
                *checked = !*checked;
                event = Some(UiEvent::ValueChanged {
                    entity,
                    id: id.clone(),
                    value: checked.to_string(),
                });
            }
            UiWidgetSpec::Slider { value, min, max } => {
                if input.primary_pressed && rect.rect.contains(input.pointer) {
                    widget.dragging = true;
                }
                if widget.dragging && input.primary_down && rect.rect.width > 0.0 {
                    let t = ((input.pointer.x - rect.rect.x) / rect.rect.width).clamp(0.0, 1.0);
                    *value = *min + t * (*max - *min);
                    event = Some(UiEvent::ValueChanged {
                        entity,
                        id: id.clone(),
                        value: format!("{value:.3}"),
                    });
                }
                if input.primary_released {
                    widget.dragging = false;
                }
            }
            UiWidgetSpec::TextBox { value, .. } if focused == Some(entity) => {
                let mut edited = value.clone();
                let mut cursor = widget.cursor.min(edited.len());
                let mut anchor = widget.anchor.min(edited.len());
                let mut changed = false;
                for key in &input.keys_pressed {
                    match key {
                        crate::input::UiKey::Character(character)
                            if !input.modifiers.ctrl && !input.modifiers.super_key =>
                        {
                            delete_selection(&mut edited, &mut cursor, &mut anchor);
                            edited.insert(cursor, *character);
                            cursor += character.len_utf8();
                            anchor = cursor;
                            changed = true;
                        }
                        crate::input::UiKey::Space
                            if !input.modifiers.ctrl && !input.modifiers.super_key =>
                        {
                            delete_selection(&mut edited, &mut cursor, &mut anchor);
                            edited.insert(cursor, ' ');
                            cursor += 1;
                            anchor = cursor;
                            changed = true;
                        }
                        crate::input::UiKey::Backspace => {
                            if selection_range(cursor, anchor).is_some() {
                                delete_selection(&mut edited, &mut cursor, &mut anchor);
                            } else if cursor > 0 {
                                let previous = prev_boundary(&edited, cursor);
                                edited.replace_range(previous..cursor, "");
                                cursor = previous;
                                anchor = cursor;
                            }
                            changed = true;
                        }
                        crate::input::UiKey::Delete => {
                            if selection_range(cursor, anchor).is_some() {
                                delete_selection(&mut edited, &mut cursor, &mut anchor);
                            } else if cursor < edited.len() {
                                let next = next_boundary(&edited, cursor);
                                edited.replace_range(cursor..next, "");
                            }
                            changed = true;
                        }
                        crate::input::UiKey::ArrowLeft => {
                            cursor = prev_boundary(&edited, cursor);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowRight => {
                            cursor = next_boundary(&edited, cursor);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::Home => {
                            cursor = 0;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::End => {
                            cursor = edited.len();
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::Character(character)
                            if input.modifiers.ctrl && character.eq_ignore_ascii_case(&'a') =>
                        {
                            anchor = 0;
                            cursor = edited.len();
                        }
                        _ => {}
                    }
                }
                if !input.text.is_empty() {
                    delete_selection(&mut edited, &mut cursor, &mut anchor);
                    edited.insert_str(cursor, &input.text);
                    cursor += input.text.len();
                    anchor = cursor;
                    changed = true;
                }
                widget.cursor = cursor;
                widget.anchor = anchor;
                if changed && edited != *value {
                    *value = edited.clone();
                    let text_size = world
                        .get_component_store::<UiResolvedStyle>()
                        .and_then(|store| {
                            store
                                .get_component(entity.index)
                                .and_then(|style| style.text.as_ref().map(|text| text.size))
                        })
                        .unwrap_or(14.0);
                    let cursor_x = edited
                        .get(..cursor.min(edited.len()))
                        .unwrap_or(&edited)
                        .chars()
                        .count() as f32
                        * text_size
                        * 0.55;
                    let visible_width = rect.content.width.max(1.0);
                    if cursor_x - widget.scroll_x > visible_width - 4.0 {
                        widget.scroll_x = cursor_x - visible_width + 4.0;
                    } else if cursor_x < widget.scroll_x {
                        widget.scroll_x = cursor_x;
                    }
                    text_changes.push((entity, edited.clone()));
                    event = Some(UiEvent::ValueChanged {
                        entity,
                        id: id.clone(),
                        value: edited,
                    });
                }
            }
            UiWidgetSpec::Dropdown {
                options,
                selected,
                open,
            } if clicks.contains(&id) => {
                let header = 38.0;
                if *open {
                    let option_index = ((input.pointer.y - (rect.rect.y + header)) / 24.0).floor();
                    if option_index >= 0.0 && (option_index as usize) < options.len() {
                        *selected = option_index as usize;
                    }
                    *open = false;
                } else {
                    *open = true;
                }
                set_widget_height(
                    world,
                    entity,
                    if *open {
                        header + options.len() as f32 * 24.0
                    } else {
                        header
                    },
                );
            }
            UiWidgetSpec::Dropdown { options, open, .. } => {
                widget.hovered_option = if *open {
                    let option_index = ((input.pointer.y - (rect.rect.y + 38.0)) / 24.0).floor();
                    (option_index >= 0.0 && (option_index as usize) < options.len())
                        .then_some(option_index as usize)
                } else {
                    None
                };
            }
            UiWidgetSpec::Tabs {
                tabs,
                active,
                contents,
            } if clicks.contains(&id) => {
                if !tabs.is_empty() && rect.rect.width > 0.0 {
                    let width = rect.rect.width / tabs.len() as f32;
                    let index = ((input.pointer.x - rect.rect.x) / width).floor();
                    if index >= 0.0 && (index as usize) < tabs.len() {
                        *active = index as usize;
                        sync_tab_contents(world, contents, *active);
                    }
                }
            }
            UiWidgetSpec::Tabs {
                active, contents, ..
            } => {
                sync_tab_contents(world, contents, *active);
            }
            UiWidgetSpec::Collapsible { open, content } if clicks.contains(&id) => {
                *open = !*open;
                if let Some(content) = content {
                    set_content_visible(world, content.clone(), *open);
                }
            }
            UiWidgetSpec::FloatingWindow { .. } => {
                let title_bar = 32.0;
                if input.primary_pressed
                    && rect.rect.contains(input.pointer)
                    && input.pointer.y <= rect.rect.y + title_bar
                {
                    widget.dragging = true;
                    widget.drag_start = Some((input.pointer.x, input.pointer.y));
                    let layout = world
                        .get_component_store::<UiLayout>()
                        .and_then(|store| store.get_component(entity.index).cloned());
                    widget.drag_base = (
                        resolve_dimension(
                            layout.as_ref().map(|layout| layout.inset.left),
                            rect.rect.x,
                        ),
                        resolve_dimension(
                            layout.as_ref().map(|layout| layout.inset.top),
                            rect.rect.y,
                        ),
                    );
                }
                if widget.dragging
                    && input.primary_down
                    && let Some((start_x, start_y)) = widget.drag_start
                {
                    move_widget(
                        world,
                        entity,
                        widget.drag_base.0 + input.pointer.x - start_x,
                        widget.drag_base.1 + input.pointer.y - start_y,
                    );
                }
                if input.primary_released {
                    widget.dragging = false;
                    widget.drag_start = None;
                }
            }
            UiWidgetSpec::Modal { open } => {
                set_entity_visible(world, entity, *open);
                if *open {
                    focus::set_modal(world, Some(entity));
                } else if focus::modal_root(world) == Some(entity) {
                    focus::set_modal(world, None);
                }
            }
            _ => {}
        }
        changes.push((
            entity,
            UiWidget {
                spec,
                dragging: widget.dragging,
                drag_start: widget.drag_start,
                drag_base: widget.drag_base,
                cursor: widget.cursor,
                anchor: widget.anchor,
                scroll_x: widget.scroll_x,
                hovered_option: widget.hovered_option,
            },
            event,
        ));
    }

    for (entity, widget, event) in changes {
        if let Some(store) = world.get_component_store_mut::<UiWidget>() {
            let store = store.get_mut_store();
            if let Some(current) = store.get_component_mut(entity.index) {
                current.spec = widget.spec;
                current.dragging = widget.dragging;
                current.drag_start = widget.drag_start;
                current.drag_base = widget.drag_base;
                current.cursor = widget.cursor;
                current.anchor = widget.anchor;
                current.scroll_x = widget.scroll_x;
                current.hovered_option = widget.hovered_option;
            }
        }
        if let Some(event) = event
            && let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>()
        {
            events.send(event);
        }
    }
    for (entity, value) in text_changes {
        let _ = world.attach_component(&entity, UiText { value });
        mark_dirty(world, entity, UiDirty::TEXT | UiDirty::LAYOUT);
    }
}

fn set_content_visible(world: &mut World, id: UiId, visible: bool) {
    let Some(entity) = world
        .get_resource::<UiForest>()
        .and_then(|forest| forest.find(&id))
    else {
        return;
    };
    set_entity_visible(world, entity, visible);
}

fn sync_tab_contents(world: &mut World, contents: &[UiId], active: usize) {
    for (index, id) in contents.iter().enumerate() {
        set_content_visible(world, id.clone(), index == active);
    }
}

fn set_entity_visible(world: &mut World, entity: Entity, visible: bool) {
    let Some(mut style) = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned())
    else {
        return;
    };
    if style.visible == Some(visible) {
        return;
    }
    style.visible = Some(visible);
    let _ = world.attach_component(&entity, style);
    mark_dirty(world, entity, UiDirty::LAYOUT | UiDirty::PAINT);
}

fn set_widget_height(world: &mut World, entity: Entity, height: f32) {
    let Some(mut layout) = world
        .get_component_store::<UiLayout>()
        .and_then(|store| store.get_component(entity.index).cloned())
    else {
        return;
    };
    layout.height = UiDimension::Length(height);
    layout.margin.bottom = UiDimension::Length(-(height - 38.0).max(0.0));
    let _ = world.attach_component(&entity, layout);
    mark_dirty(world, entity, UiDirty::LAYOUT | UiDirty::PAINT);
}

fn resolve_dimension(value: Option<UiDimension>, fallback: f32) -> f32 {
    match value {
        Some(UiDimension::Length(value)) => value,
        Some(UiDimension::Percent(value)) => fallback * value / 100.0,
        _ => fallback,
    }
}

fn move_widget(world: &mut World, entity: Entity, x: f32, y: f32) {
    let Some(mut layout) = world
        .get_component_store::<UiLayout>()
        .and_then(|store| store.get_component(entity.index).cloned())
    else {
        return;
    };
    layout.position = UiPosition::Absolute;
    layout.inset.left = UiDimension::Length(x);
    layout.inset.top = UiDimension::Length(y);
    let _ = world.attach_component(&entity, layout);
    mark_dirty(world, entity, UiDirty::LAYOUT | UiDirty::PAINT);
}

/// Appends widget-specific paint commands after the generic draw list.
pub fn append_widget_commands(world: &World, draw: &mut UiDrawList) {
    let Some(widgets) = world.get_component_store::<UiWidget>() else {
        return;
    };
    let computed = world.get_component_store::<UiComputed>();
    let styles = world.get_component_store::<UiResolvedStyle>();
    let focused = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);

    for entity in world.entities() {
        let Some(widget) = widgets.get_component(entity.index) else {
            continue;
        };
        let Some(rect) = computed
            .as_ref()
            .and_then(|store| store.get_component(entity.index).copied())
        else {
            continue;
        };
        if !rect.visible {
            continue;
        }
        let style = styles
            .as_ref()
            .and_then(|store| store.get_component(entity.index).cloned())
            .unwrap_or_default();
        let clip = rect.clip;
        match &widget.spec {
            UiWidgetSpec::Toggle { on } => {
                let track_w = 40.0;
                let track_h = 22.0;
                let track = UiRect::new(
                    rect.rect.x + (rect.rect.width - track_w).max(0.0) * 0.5,
                    rect.rect.y + (rect.rect.height - track_h).max(0.0) * 0.5,
                    track_w,
                    track_h,
                );
                let fill = if *on {
                    [0.20, 0.62, 0.44, 1.0]
                } else {
                    [0.22, 0.25, 0.32, 1.0]
                };
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: track,
                    color: fill,
                    radius: [11.0; 4],
                    clip,
                });
                let knob_x = if *on { track.x + 22.0 } else { track.x + 2.0 };
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect::new(knob_x, track.y + 2.0, 18.0, 18.0),
                    color: [0.94, 0.96, 1.0, 1.0],
                    radius: [9.0; 4],
                    clip,
                });
            }
            UiWidgetSpec::Checkbox { checked } => {
                let box_size = 18.0;
                let box_rect = UiRect::new(
                    rect.rect.x + (rect.rect.width - box_size).max(0.0) * 0.5,
                    rect.rect.y + (rect.rect.height - box_size).max(0.0) * 0.5,
                    box_size,
                    box_size,
                );
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: box_rect,
                    color: [0.12, 0.14, 0.19, 1.0],
                    radius: [4.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Border {
                    entity,
                    rect: box_rect,
                    width: 1.0,
                    color: [0.40, 0.45, 0.55, 1.0],
                    radius: [4.0; 4],
                    clip,
                });
                if *checked {
                    draw.commands.push(UiDrawCommand::Rect {
                        entity,
                        rect: UiRect::new(
                            box_rect.x + 4.0,
                            box_rect.y + 4.0,
                            box_rect.width - 8.0,
                            box_rect.height - 8.0,
                        ),
                        color: [0.30, 0.75, 0.52, 1.0],
                        radius: [2.0; 4],
                        clip,
                    });
                }
            }
            UiWidgetSpec::Slider { value, min, max } => {
                let t = ((*value - *min) / (*max - *min).max(f32::EPSILON)).clamp(0.0, 1.0);
                let track = UiRect::new(
                    rect.rect.x,
                    rect.rect.y + rect.rect.height * 0.5 - 3.0,
                    rect.rect.width,
                    6.0,
                );
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: track,
                    color: [0.20, 0.23, 0.30, 1.0],
                    radius: [3.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect {
                        x: track.x,
                        y: track.y,
                        width: track.width * t,
                        height: track.height,
                    },
                    color: [0.26, 0.62, 0.92, 1.0],
                    radius: [3.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect::new(track.x + track.width * t - 7.0, track.y - 5.0, 14.0, 16.0),
                    color: [0.94, 0.96, 1.0, 1.0],
                    radius: [7.0; 4],
                    clip,
                });
            }
            UiWidgetSpec::ProgressBar { value } => {
                let track = UiRect::new(
                    rect.rect.x,
                    rect.rect.y + rect.rect.height * 0.5 - 4.0,
                    rect.rect.width,
                    8.0,
                );
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: track,
                    color: [0.18, 0.21, 0.28, 1.0],
                    radius: [4.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect {
                        x: track.x,
                        y: track.y,
                        width: track.width * value.clamp(0.0, 1.0),
                        height: track.height,
                    },
                    color: [0.30, 0.70, 0.50, 1.0],
                    radius: [4.0; 4],
                    clip,
                });
            }
            UiWidgetSpec::TextBox { value, .. } if focused == Some(entity) => {
                let size = style.text.as_ref().map_or(14.0, |text| text.size);
                let line_height = style
                    .text
                    .as_ref()
                    .and_then(|text| text.line_height)
                    .unwrap_or(size * 1.25);
                let cursor_chars = value
                    .get(..widget.cursor.min(value.len()))
                    .unwrap_or(value)
                    .chars()
                    .count();
                let caret_x = (rect.content.x + cursor_chars as f32 * size * 0.55
                    - widget.scroll_x)
                    .clamp(rect.content.x, rect.content.x + rect.content.width - 2.0);
                let caret_y = rect.content.y + (rect.content.height - line_height).max(0.0) * 0.5;
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: UiRect::new(caret_x, caret_y, 1.5, line_height),
                    color: [0.92, 0.94, 1.0, 0.9],
                    clip,
                });
            }
            UiWidgetSpec::Dropdown {
                options,
                selected,
                open,
            } => {
                let arrow = UiRect::new(
                    rect.rect.x + rect.rect.width - 22.0,
                    rect.rect.y + 15.0,
                    8.0,
                    8.0,
                );
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: arrow,
                    color: [0.78, 0.82, 0.92, 1.0],
                    radius: [2.0; 4],
                    clip,
                });
                let text_style = style.text.clone().unwrap_or_else(default_text);
                if let Some(option) = options.get(*selected) {
                    draw.commands.push(UiDrawCommand::Text {
                        entity,
                        rect: UiRect::new(
                            rect.rect.x + 10.0,
                            rect.rect.y + 9.0,
                            rect.rect.width - 34.0,
                            20.0,
                        ),
                        text: option.clone(),
                        style: text_style.clone(),
                        clip,
                    });
                }
                if *open {
                    for (index, option) in options.iter().enumerate() {
                        let y = rect.rect.y + 38.0 + index as f32 * 24.0;
                        if index == *selected || Some(index) == widget.hovered_option {
                            let color = if Some(index) == widget.hovered_option {
                                [0.26, 0.40, 0.58, 1.0]
                            } else {
                                [0.20, 0.32, 0.48, 1.0]
                            };
                            draw.commands.push(UiDrawCommand::Rect {
                                entity,
                                rect: UiRect::new(rect.rect.x, y, rect.rect.width, 22.0),
                                color,
                                radius: [3.0; 4],
                                clip,
                            });
                        }
                        draw.commands.push(UiDrawCommand::Text {
                            entity,
                            rect: UiRect::new(
                                rect.rect.x + 10.0,
                                y + 2.0,
                                rect.rect.width - 20.0,
                                20.0,
                            ),
                            text: option.clone(),
                            style: text_style.clone(),
                            clip,
                        });
                    }
                }
            }
            UiWidgetSpec::ListBox { items, selected } => {
                let text_style = style.text.clone().unwrap_or_else(default_text);
                for (index, item) in items.iter().take(12).enumerate() {
                    let y = rect.content.y + index as f32 * 22.0;
                    if y > rect.content.y + rect.content.height {
                        break;
                    }
                    if index == *selected {
                        draw.commands.push(UiDrawCommand::Rect {
                            entity,
                            rect: UiRect::new(rect.content.x, y, rect.content.width, 20.0),
                            color: [0.20, 0.32, 0.48, 1.0],
                            radius: [3.0; 4],
                            clip,
                        });
                    }
                    draw.commands.push(UiDrawCommand::Text {
                        entity,
                        rect: UiRect::new(
                            rect.content.x + 6.0,
                            y + 1.0,
                            rect.content.width - 12.0,
                            20.0,
                        ),
                        text: item.clone(),
                        style: text_style.clone(),
                        clip,
                    });
                }
            }
            UiWidgetSpec::Tabs { tabs, active, .. } => {
                if tabs.is_empty() || rect.rect.width <= 0.0 {
                    continue;
                }
                let tab_width = rect.rect.width / tabs.len() as f32;
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: UiRect::new(
                        rect.rect.x,
                        rect.rect.y + rect.rect.height - 2.0,
                        rect.rect.width,
                        2.0,
                    ),
                    color: [0.28, 0.31, 0.40, 1.0],
                    clip,
                });
                let active_rect = UiRect::new(
                    rect.rect.x + *active as f32 * tab_width,
                    rect.rect.y + rect.rect.height - 3.0,
                    tab_width,
                    3.0,
                );
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: active_rect,
                    color: [0.30, 0.68, 0.96, 1.0],
                    clip,
                });
                let text_style = style.text.clone().unwrap_or_else(default_text);
                for (index, tab) in tabs.iter().enumerate() {
                    draw.commands.push(UiDrawCommand::Text {
                        entity,
                        rect: UiRect::new(
                            rect.rect.x + index as f32 * tab_width + 6.0,
                            rect.content.y + (rect.content.height - 22.0).max(0.0) * 0.5,
                            tab_width - 12.0,
                            rect.content.height,
                        ),
                        text: tab.clone(),
                        style: text_style.clone(),
                        clip,
                    });
                }
            }
            UiWidgetSpec::FloatingWindow { title, .. } => {
                let title_style = style.text.clone().unwrap_or_else(default_text);
                draw.commands.push(UiDrawCommand::Text {
                    entity,
                    rect: UiRect::new(
                        rect.rect.x + 10.0,
                        rect.rect.y + 6.0,
                        rect.rect.width - 20.0,
                        22.0,
                    ),
                    text: title.clone(),
                    style: title_style,
                    clip,
                });
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: UiRect::new(rect.rect.x, rect.rect.y + 30.0, rect.rect.width, 1.0),
                    color: [0.24, 0.27, 0.36, 1.0],
                    clip,
                });
            }
            UiWidgetSpec::Sparkline { values, max } => {
                if values.is_empty() || rect.content.width <= 0.0 {
                    continue;
                }
                let max = if *max > 0.0 {
                    *max
                } else {
                    values.iter().copied().fold(0.0, f32::max).max(0.001)
                };
                let step = rect.content.width / values.len() as f32;
                for (index, value) in values.iter().enumerate() {
                    let height = (value / max).clamp(0.0, 1.0) * rect.content.height;
                    draw.commands.push(UiDrawCommand::Rect {
                        entity,
                        rect: UiRect::new(
                            rect.content.x + index as f32 * step,
                            rect.content.y + rect.content.height - height,
                            step.max(1.0),
                            height,
                        ),
                        color: [0.30, 0.70, 0.96, 1.0],
                        radius: [0.0; 4],
                        clip,
                    });
                }
            }
            UiWidgetSpec::Divider => {
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: UiRect::new(
                        rect.rect.x,
                        rect.rect.y + rect.rect.height * 0.5,
                        rect.rect.width,
                        1.0,
                    ),
                    color: [0.24, 0.27, 0.36, 1.0],
                    clip,
                });
            }
            UiWidgetSpec::Modal { open } if *open => {
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect::new(0.0, 0.0, 10_000.0, 10_000.0),
                    color: [0.0, 0.0, 0.0, 0.35],
                    radius: [0.0; 4],
                    clip,
                });
            }
            _ => {}
        }
    }
}

fn default_text() -> UiResolvedTextStyle {
    UiResolvedTextStyle {
        size: 14.0,
        line_height: None,
        color: [0.90, 0.93, 1.0, 1.0],
        family: None,
        weight: 400,
        wrap: UiTextWrap::Word,
        ellipsis: false,
    }
}

fn mark_dirty(world: &mut World, entity: Entity, dirty: UiDirty) {
    if let Some(store) = world.get_component_store_mut::<UiDirty>() {
        let store = store.get_mut_store();
        let next = store
            .get_component(entity.index)
            .copied()
            .unwrap_or_default()
            | dirty;
        store.attach(entity.index, next);
    }
}

fn selection_range(cursor: usize, anchor: usize) -> Option<(usize, usize)> {
    (cursor != anchor).then_some((cursor.min(anchor), cursor.max(anchor)))
}

fn delete_selection(text: &mut String, cursor: &mut usize, anchor: &mut usize) {
    if let Some((start, end)) = selection_range(*cursor, *anchor) {
        text.replace_range(start..end, "");
        *cursor = start;
        *anchor = start;
    }
}

fn prev_boundary(text: &str, index: usize) -> usize {
    text[..index.min(text.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(offset, _)| offset)
}

fn next_boundary(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text[index..]
        .char_indices()
        .nth(1)
        .map_or(text.len(), |(offset, _)| index + offset)
}
