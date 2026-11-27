//! v1 widget state machines and paint commands.
//!
//! Widgets are ordinary ECS nodes with a [`UiWidget`] component. Interaction
//! is driven by the shared [`UiInputState`] and [`UiEvent`] stream, so widgets
//! never store closures.

use std::collections::HashSet;

use ignit_ecs::{Entity, Events, World};
use serde::{Deserialize, Serialize};

use crate::clipboard::UiClipboardResource;
use crate::draw::{UiDrawCommand, UiDrawList};
use crate::events::UiEvent;
use crate::focus::{self, UiFocusState};
use crate::forest::UiForest;
use crate::geometry::UiRect;
use crate::id::UiId;
use crate::input::UiInputState;
use crate::layout::{UiDimension, UiLayout, UiPosition};
use crate::layout_engine::UiTextMeasure;
use crate::node::{UiComputed, UiDirty};
use crate::style::{
    UiColor, UiResolvedStyle, UiResolvedTextStyle, UiStyle, UiText, UiTextStyle, UiTextWrap,
    UiThemes,
};

/// Smallest width a floating window may be resized to, in logical pixels.
pub const MIN_FLOATING_WINDOW_WIDTH: f32 = 240.0;
/// Smallest height a floating window may be resized to, in logical pixels.
pub const MIN_FLOATING_WINDOW_HEIGHT: f32 = 160.0;

/// One line of a multi-series [`UiWidgetSpec::Graph`].
///
/// Values are drawn oldest first. A series with a transparent colour falls
/// back to the widget's palette, indexed by its position.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiGraphSeries {
    pub values: Vec<f32>,
    #[serde(default)]
    pub color: [f32; 4],
}

impl UiGraphSeries {
    /// A series drawn in `color`.
    pub fn new(values: Vec<f32>, color: [f32; 4]) -> Self {
        Self { values, color }
    }
}

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
    RadioGroup {
        options: Vec<String>,
        selected: usize,
        horizontal: bool,
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
    /// Several overlaid series, drawn as narrow bars.
    Graph {
        series: Vec<UiGraphSeries>,
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
    pub scroll_y: f32,
    /// Column target while moving between visual lines.
    pub goal_x: Option<f32>,
    pub hovered_option: Option<usize>,
    pub text_drag: bool,
    pub resizing: bool,
    pub resize_start: Option<(f32, f32)>,
    pub resize_base: (f32, f32),
    /// Monotonic raise offset for floating-window stacking.
    pub raise: i32,
}

/// Allocates stable raise offsets so a clicked floating window stays above
/// windows raised earlier.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiWindowOrder {
    pub next: i32,
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
            scroll_y: 0.0,
            goal_x: None,
            hovered_option: None,
            text_drag: false,
            resizing: false,
            resize_start: None,
            resize_base: (0.0, 0.0),
            raise: 0,
        }
    }
}

/// Runs widget state changes. Call after hit testing/focus and before the draw
/// list is built.
pub fn update_widgets(world: &mut World, input: &UiInputState, measure: &mut dyn UiTextMeasure) {
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
            UiWidgetSpec::TextBox {
                value, multiline, ..
            } if focused == Some(entity) => {
                let mut edited = value.clone();
                let mut cursor = widget.cursor.min(edited.len());
                let mut anchor = widget.anchor.min(edited.len());
                let mut changed = false;
                let edit_style = edit_text_style(world, entity);
                let content_width = rect.content.width.max(1.0);
                let content_height = rect.content.height.max(1.0);
                let text_width = measure.measure(&edited, &edit_style, None).width;
                let text_height = measure
                    .measure(&edited, &edit_style, Some(content_width))
                    .height;
                let max_scroll_x = if !*multiline || edit_style.wrap == UiTextWrap::NoWrap {
                    (text_width - content_width).max(0.0)
                } else {
                    0.0
                };
                let max_scroll_y = (text_height - content_height).max(0.0);

                if widget.text_drag && input.primary_down {
                    let over_right = input.pointer.x - (rect.content.x + content_width);
                    let over_left = input.pointer.x - rect.content.x;
                    if over_right > 0.0 {
                        widget.scroll_x += (over_right * 0.25).max(1.0);
                    } else if over_left < 0.0 {
                        widget.scroll_x += (over_left * 0.25).min(-1.0);
                    }
                    let over_bottom = input.pointer.y - (rect.content.y + content_height);
                    let over_top = input.pointer.y - rect.content.y;
                    if over_bottom > 0.0 {
                        widget.scroll_y += (over_bottom * 0.25).max(1.0);
                    } else if over_top < 0.0 {
                        widget.scroll_y += (over_top * 0.25).min(-1.0);
                    }
                    widget.scroll_x = widget.scroll_x.clamp(0.0, max_scroll_x);
                    widget.scroll_y = widget.scroll_y.clamp(0.0, max_scroll_y);
                }

                let pointer_x = input.pointer.x - rect.content.x + widget.scroll_x;
                let pointer_y = input.pointer.y - rect.content.y + widget.scroll_y;
                if input.primary_pressed && rect.rect.contains(input.pointer) {
                    let next = measure.cursor_index(
                        &edited,
                        &edit_style,
                        content_width,
                        pointer_x,
                        pointer_y,
                    );
                    cursor = next;
                    if !input.modifiers.shift {
                        anchor = next;
                    }
                    widget.text_drag = true;
                    widget.goal_x = None;
                } else if widget.text_drag && input.primary_down {
                    cursor = measure.cursor_index(
                        &edited,
                        &edit_style,
                        content_width,
                        pointer_x,
                        pointer_y,
                    );
                }
                if input.primary_released {
                    widget.text_drag = false;
                }

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
                        crate::input::UiKey::Enter if *multiline => {
                            delete_selection(&mut edited, &mut cursor, &mut anchor);
                            edited.insert(cursor, '\n');
                            cursor += 1;
                            anchor = cursor;
                            changed = true;
                        }
                        crate::input::UiKey::ArrowLeft
                            if input.modifiers.ctrl || input.modifiers.super_key =>
                        {
                            cursor = prev_word_boundary(&edited, cursor);
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowRight
                            if input.modifiers.ctrl || input.modifiers.super_key =>
                        {
                            cursor = next_word_boundary(&edited, cursor);
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowLeft => {
                            cursor = prev_boundary(&edited, cursor);
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowRight => {
                            cursor = next_boundary(&edited, cursor);
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowUp => {
                            let goal = widget.goal_x.unwrap_or_else(|| {
                                measure
                                    .cursor_rect(&edited, &edit_style, content_width, cursor)
                                    .map_or(0.0, |rect| rect[0])
                            });
                            cursor = measure.vertical_cursor(
                                &edited,
                                &edit_style,
                                content_width,
                                cursor,
                                true,
                                goal,
                            );
                            widget.goal_x = Some(goal);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::ArrowDown => {
                            let goal = widget.goal_x.unwrap_or_else(|| {
                                measure
                                    .cursor_rect(&edited, &edit_style, content_width, cursor)
                                    .map_or(0.0, |rect| rect[0])
                            });
                            cursor = measure.vertical_cursor(
                                &edited,
                                &edit_style,
                                content_width,
                                cursor,
                                false,
                                goal,
                            );
                            widget.goal_x = Some(goal);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::Home => {
                            let (start, _) =
                                measure.line_bounds(&edited, &edit_style, content_width, cursor);
                            cursor = start;
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::End => {
                            let (_, end) =
                                measure.line_bounds(&edited, &edit_style, content_width, cursor);
                            cursor = end;
                            widget.goal_x = None;
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::PageUp => {
                            let goal = widget.goal_x.unwrap_or_else(|| {
                                measure
                                    .cursor_rect(&edited, &edit_style, content_width, cursor)
                                    .map_or(0.0, |rect| rect[0])
                            });
                            let line_height = edit_style
                                .line_height
                                .unwrap_or(edit_style.size * 1.25)
                                .max(1.0);
                            let page = (content_height / line_height).floor().max(1.0) as usize;
                            for _ in 0..page {
                                cursor = measure.vertical_cursor(
                                    &edited,
                                    &edit_style,
                                    content_width,
                                    cursor,
                                    true,
                                    goal,
                                );
                            }
                            widget.goal_x = Some(goal);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::PageDown => {
                            let goal = widget.goal_x.unwrap_or_else(|| {
                                measure
                                    .cursor_rect(&edited, &edit_style, content_width, cursor)
                                    .map_or(0.0, |rect| rect[0])
                            });
                            let line_height = edit_style
                                .line_height
                                .unwrap_or(edit_style.size * 1.25)
                                .max(1.0);
                            let page = (content_height / line_height).floor().max(1.0) as usize;
                            for _ in 0..page {
                                cursor = measure.vertical_cursor(
                                    &edited,
                                    &edit_style,
                                    content_width,
                                    cursor,
                                    false,
                                    goal,
                                );
                            }
                            widget.goal_x = Some(goal);
                            if !input.modifiers.shift {
                                anchor = cursor;
                            }
                        }
                        crate::input::UiKey::Character(character)
                            if (input.modifiers.ctrl || input.modifiers.super_key)
                                && character.eq_ignore_ascii_case(&'a') =>
                        {
                            anchor = 0;
                            cursor = edited.len();
                            widget.goal_x = None;
                        }
                        crate::input::UiKey::Character(character)
                            if (input.modifiers.ctrl || input.modifiers.super_key)
                                && character.eq_ignore_ascii_case(&'c') =>
                        {
                            if let Some((start, end)) = selection_range(cursor, anchor)
                                && let Some(clipboard) = world.get_resource::<UiClipboardResource>()
                            {
                                clipboard.set_text(edited[start..end].to_string());
                            }
                        }
                        crate::input::UiKey::Character(character)
                            if (input.modifiers.ctrl || input.modifiers.super_key)
                                && character.eq_ignore_ascii_case(&'x') =>
                        {
                            if let Some((start, end)) = selection_range(cursor, anchor) {
                                if let Some(clipboard) = world.get_resource::<UiClipboardResource>()
                                {
                                    clipboard.set_text(edited[start..end].to_string());
                                }
                                delete_selection(&mut edited, &mut cursor, &mut anchor);
                                changed = true;
                            }
                        }
                        crate::input::UiKey::Character(character)
                            if (input.modifiers.ctrl || input.modifiers.super_key)
                                && character.eq_ignore_ascii_case(&'v') =>
                        {
                            let pasted = world
                                .get_resource::<UiClipboardResource>()
                                .and_then(|clipboard| clipboard.get_text());
                            if let Some(mut pasted) = pasted {
                                if !*multiline {
                                    pasted = pasted.replace(['\n', '\r'], " ");
                                }
                                delete_selection(&mut edited, &mut cursor, &mut anchor);
                                edited.insert_str(cursor, &pasted);
                                cursor += pasted.len();
                                anchor = cursor;
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
                if !input.text.is_empty() && !input.modifiers.ctrl && !input.modifiers.super_key {
                    delete_selection(&mut edited, &mut cursor, &mut anchor);
                    edited.insert_str(cursor, &input.text);
                    cursor += input.text.len();
                    anchor = cursor;
                    changed = true;
                }
                if changed {
                    widget.goal_x = None;
                }
                widget.cursor = cursor;
                widget.anchor = anchor;
                if let Some(caret) =
                    measure.cursor_rect(&edited, &edit_style, content_width, cursor)
                {
                    if caret[0] - widget.scroll_x > content_width - 4.0 {
                        widget.scroll_x = caret[0] - content_width + 4.0;
                    } else if caret[0] < widget.scroll_x {
                        widget.scroll_x = caret[0];
                    }
                    if caret[1] + caret[3] - widget.scroll_y > content_height - 4.0 {
                        widget.scroll_y = caret[1] + caret[3] - content_height + 4.0;
                    } else if caret[1] < widget.scroll_y {
                        widget.scroll_y = caret[1];
                    }
                }
                let text_width = measure.measure(&edited, &edit_style, None).width;
                let text_height = measure
                    .measure(&edited, &edit_style, Some(content_width))
                    .height;
                let max_scroll_x = if !*multiline || edit_style.wrap == UiTextWrap::NoWrap {
                    (text_width - content_width).max(0.0)
                } else {
                    0.0
                };
                widget.scroll_x = widget.scroll_x.clamp(0.0, max_scroll_x).round();
                widget.scroll_y = widget
                    .scroll_y
                    .clamp(0.0, (text_height - content_height).max(0.0))
                    .round();
                if changed && edited != *value {
                    *value = edited.clone();
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
                if *open && input.primary_pressed && !rect.rect.contains(input.pointer) {
                    *open = false;
                    set_widget_height(world, entity, 38.0);
                }
                widget.hovered_option = if *open {
                    let option_index = ((input.pointer.y - (rect.rect.y + 38.0)) / 24.0).floor();
                    (option_index >= 0.0 && (option_index as usize) < options.len())
                        .then_some(option_index as usize)
                } else {
                    None
                };
            }
            UiWidgetSpec::RadioGroup {
                options,
                selected,
                horizontal,
            } if clicks.contains(&id) => {
                if !options.is_empty() {
                    let index = if *horizontal {
                        ((input.pointer.x - rect.rect.x) / (rect.rect.width / options.len() as f32))
                            .floor()
                    } else {
                        ((input.pointer.y - rect.rect.y)
                            / (rect.rect.height / options.len() as f32))
                            .floor()
                    };
                    if index >= 0.0 && (index as usize) < options.len() {
                        *selected = index as usize;
                        event = Some(UiEvent::ValueChanged {
                            entity,
                            id: id.clone(),
                            value: selected.to_string(),
                        });
                    }
                }
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
            UiWidgetSpec::FloatingWindow { open, .. } => {
                set_entity_visible(world, entity, *open);
                let title_bar = 32.0;
                let grip = 18.0;
                let on_grip = input.pointer.x >= rect.rect.x + rect.rect.width - grip
                    && input.pointer.y >= rect.rect.y + rect.rect.height - grip;
                if input.primary_pressed && rect.rect.contains(input.pointer) {
                    raise_floating(world, entity, &mut widget);
                    if on_grip {
                        widget.resizing = true;
                        widget.resize_start = Some((input.pointer.x, input.pointer.y));
                        widget.resize_base = (rect.rect.width, rect.rect.height);
                        widget.dragging = false;
                    } else if input.pointer.y <= rect.rect.y + title_bar {
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
                if widget.resizing
                    && input.primary_down
                    && let Some((start_x, start_y)) = widget.resize_start
                {
                    resize_widget(
                        world,
                        entity,
                        widget.resize_base.0 + input.pointer.x - start_x,
                        widget.resize_base.1 + input.pointer.y - start_y,
                    );
                }
                if input.primary_released {
                    let was_interacting = widget.dragging || widget.resizing;
                    widget.dragging = false;
                    widget.drag_start = None;
                    widget.resizing = false;
                    widget.resize_start = None;
                    if was_interacting {
                        // The viewport clamp is skipped while the pointer owns
                        // the window. Re-run layout once the interaction ends so
                        // an off-screen position or oversize is pulled back.
                        mark_dirty(world, entity, UiDirty::LAYOUT);
                    }
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
                scroll_y: widget.scroll_y,
                goal_x: widget.goal_x,
                hovered_option: widget.hovered_option,
                text_drag: widget.text_drag,
                resizing: widget.resizing,
                resize_start: widget.resize_start,
                resize_base: widget.resize_base,
                raise: widget.raise,
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
                current.scroll_y = widget.scroll_y;
                current.goal_x = widget.goal_x;
                current.hovered_option = widget.hovered_option;
                current.text_drag = widget.text_drag;
                current.resizing = widget.resizing;
                current.resize_start = widget.resize_start;
                current.resize_base = widget.resize_base;
                current.raise = widget.raise;
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

fn raise_floating(world: &mut World, entity: Entity, widget: &mut UiWidget) {
    let next = if let Some(mut order) = world.get_resource_mut::<UiWindowOrder>() {
        order.next = order.next.saturating_add(10_000);
        order.next
    } else {
        widget.raise.saturating_add(10_000)
    };
    if widget.raise != next {
        widget.raise = next;
        mark_dirty(world, entity, UiDirty::LAYOUT);
    }
}

fn resize_widget(world: &mut World, entity: Entity, width: f32, height: f32) {
    let Some(mut layout) = world
        .get_component_store::<UiLayout>()
        .and_then(|store| store.get_component(entity.index).cloned())
    else {
        return;
    };
    layout.width = UiDimension::Length(width.max(MIN_FLOATING_WINDOW_WIDTH));
    layout.height = UiDimension::Length(height.max(MIN_FLOATING_WINDOW_HEIGHT));
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
    let themes = world.get_resource::<UiThemes>();
    let theme = themes.as_deref();

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
                    token_color(theme, "accent", [0.20, 0.62, 0.44, 1.0])
                } else {
                    token_color(theme, "border", [0.22, 0.25, 0.32, 1.0])
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
                    color: token_color(theme, "surface", [0.94, 0.96, 1.0, 1.0]),
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
                    color: token_color(theme, "surface_alt", [0.12, 0.14, 0.19, 1.0]),
                    radius: [4.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Border {
                    entity,
                    rect: box_rect,
                    width: 1.0,
                    color: token_color(theme, "border", [0.40, 0.45, 0.55, 1.0]),
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
                        color: token_color(theme, "accent", [0.30, 0.75, 0.52, 1.0]),
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
                    color: token_color(theme, "border", [0.20, 0.23, 0.30, 1.0]),
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
                    color: token_color(theme, "accent", [0.26, 0.62, 0.92, 1.0]),
                    radius: [3.0; 4],
                    clip,
                });
                let knob = UiRect::new(track.x + track.width * t - 7.0, track.y - 5.0, 14.0, 16.0);
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: knob,
                    color: token_color(theme, "surface", [0.94, 0.96, 1.0, 1.0]),
                    radius: [7.0; 4],
                    clip,
                });
                draw.commands.push(UiDrawCommand::Border {
                    entity,
                    rect: knob,
                    width: 1.0,
                    color: token_color(theme, "border", [0.40, 0.45, 0.55, 1.0]),
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
                    color: token_color(theme, "border", [0.18, 0.21, 0.28, 1.0]),
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
                    color: token_color(theme, "accent", [0.30, 0.70, 0.50, 1.0]),
                    radius: [4.0; 4],
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
                    color: token_color(theme, "text_muted", [0.78, 0.82, 0.92, 1.0]),
                    radius: [2.0; 4],
                    clip,
                });
                let text_style = style.text.clone().unwrap_or_else(|| default_text(theme));
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
                        caret: None,
                        selection: None,
                        clip,
                    });
                }
                if *open {
                    for (index, option) in options.iter().enumerate() {
                        let y = rect.rect.y + 38.0 + index as f32 * 24.0;
                        if index == *selected || Some(index) == widget.hovered_option {
                            let color = if Some(index) == widget.hovered_option {
                                with_alpha(
                                    token_color(theme, "accent_hover", [0.34, 0.70, 0.98, 1.0]),
                                    0.38,
                                )
                            } else {
                                with_alpha(
                                    token_color(theme, "accent", [0.26, 0.62, 0.92, 1.0]),
                                    0.28,
                                )
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
                            caret: None,
                            selection: None,
                            clip,
                        });
                    }
                }
            }
            UiWidgetSpec::RadioGroup {
                options,
                selected,
                horizontal,
            } => {
                let text_style = style.text.clone().unwrap_or_else(|| default_text(theme));
                let count = options.len().max(1) as f32;
                let (cell_w, cell_h) = if *horizontal {
                    (rect.content.width / count, rect.content.height)
                } else {
                    (rect.content.width, rect.content.height / count)
                };
                for (index, option) in options.iter().enumerate() {
                    let x = if *horizontal {
                        rect.content.x + index as f32 * cell_w
                    } else {
                        rect.content.x
                    };
                    let y = if *horizontal {
                        rect.content.y
                    } else {
                        rect.content.y + index as f32 * cell_h
                    };
                    let circle = UiRect::new(x + 2.0, y + (cell_h - 14.0) * 0.5, 14.0, 14.0);
                    draw.commands.push(UiDrawCommand::Border {
                        entity,
                        rect: circle,
                        width: 1.0,
                        color: token_color(theme, "border", [0.4, 0.45, 0.55, 1.0]),
                        radius: [7.0; 4],
                        clip,
                    });
                    if index == *selected {
                        draw.commands.push(UiDrawCommand::Rect {
                            entity,
                            rect: UiRect::new(circle.x + 4.0, circle.y + 4.0, 6.0, 6.0),
                            color: token_color(theme, "accent", [0.26, 0.62, 0.92, 1.0]),
                            radius: [3.0; 4],
                            clip,
                        });
                    }
                    draw.commands.push(UiDrawCommand::Text {
                        entity,
                        rect: UiRect::new(x + 22.0, y + (cell_h - 16.0) * 0.5, cell_w - 24.0, 16.0),
                        text: option.clone(),
                        style: text_style.clone(),
                        caret: None,
                        selection: None,
                        clip,
                    });
                }
            }
            UiWidgetSpec::ListBox { items, selected } => {
                let text_style = style.text.clone().unwrap_or_else(|| default_text(theme));
                for (index, item) in items.iter().take(12).enumerate() {
                    let y = rect.content.y + index as f32 * 22.0;
                    if y > rect.content.y + rect.content.height {
                        break;
                    }
                    if index == *selected {
                        draw.commands.push(UiDrawCommand::Rect {
                            entity,
                            rect: UiRect::new(rect.content.x, y, rect.content.width, 20.0),
                            color: with_alpha(
                                token_color(theme, "accent", [0.26, 0.62, 0.92, 1.0]),
                                0.28,
                            ),
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
                        caret: None,
                        selection: None,
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
                    color: token_color(theme, "border", [0.28, 0.31, 0.40, 1.0]),
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
                    color: token_color(theme, "accent", [0.30, 0.68, 0.96, 1.0]),
                    clip,
                });
                let text_style = style.text.clone().unwrap_or_else(|| default_text(theme));
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
                        caret: None,
                        selection: None,
                        clip,
                    });
                }
            }
            UiWidgetSpec::FloatingWindow { title, .. } => {
                let title_style = style.text.clone().unwrap_or_else(|| default_text(theme));
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
                    caret: None,
                    selection: None,
                    clip,
                });
                draw.commands.push(UiDrawCommand::Line {
                    entity,
                    rect: UiRect::new(rect.rect.x, rect.rect.y + 30.0, rect.rect.width, 1.0),
                    color: token_color(theme, "border", [0.24, 0.27, 0.36, 1.0]),
                    clip,
                });
                let grip = token_color(theme, "border", [0.24, 0.27, 0.36, 1.0]);
                for index in 0..3 {
                    let offset = 5.0 + index as f32 * 4.0;
                    let length = 5.0 + index as f32 * 5.0;
                    draw.commands.push(UiDrawCommand::Line {
                        entity,
                        rect: UiRect::new(
                            rect.rect.x + rect.rect.width - offset - length,
                            rect.rect.y + rect.rect.height - offset,
                            length,
                            1.0,
                        ),
                        color: grip,
                        clip,
                    });
                }
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
                let bar = (step - 1.0).clamp(1.0, 3.0);
                for (index, value) in values.iter().enumerate() {
                    let height = (value / max).clamp(0.0, 1.0) * rect.content.height;
                    draw.commands.push(UiDrawCommand::Rect {
                        entity,
                        rect: UiRect::new(
                            rect.content.x + index as f32 * step + (step - bar).max(0.0) * 0.5,
                            rect.content.y + rect.content.height - height,
                            bar,
                            height,
                        ),
                        color: token_color(theme, "accent", [0.30, 0.70, 0.96, 1.0]),
                        radius: [0.0; 4],
                        clip,
                    });
                }
            }
            UiWidgetSpec::Graph { series, max } => {
                let samples = series
                    .iter()
                    .map(|series| series.values.len())
                    .max()
                    .unwrap_or(0);
                if samples == 0 || rect.content.width <= 0.0 || rect.content.height <= 0.0 {
                    continue;
                }

                let peak = series
                    .iter()
                    .flat_map(|series| series.values.iter().copied())
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .fold(0.0f32, f32::max);
                let max = if max.is_finite() && *max > 0.0 {
                    *max
                } else {
                    peak.max(0.001)
                };

                let step = rect.content.width / samples as f32;
                let lanes = series.len().max(1) as f32;
                let grouped = (step - 1.0).max(1.0);
                let bar = if series.len() > 1 {
                    (grouped / lanes).clamp(1.0, 3.0)
                } else {
                    grouped.clamp(1.0, 3.0)
                };
                let group_width = bar * lanes;
                let offset = ((step - group_width) * 0.5).max(0.0);

                for (series_index, series) in series.iter().enumerate() {
                    let color = if series.color[3] > 0.0 {
                        series.color
                    } else {
                        graph_palette(series_index)
                    };
                    for (index, value) in series.values.iter().enumerate() {
                        if !value.is_finite() || *value < 0.0 {
                            continue;
                        }
                        let height = (value / max).clamp(0.0, 1.0) * rect.content.height;
                        draw.commands.push(UiDrawCommand::Rect {
                            entity,
                            rect: UiRect::new(
                                rect.content.x
                                    + index as f32 * step
                                    + offset
                                    + series_index as f32 * bar,
                                rect.content.y + rect.content.height - height,
                                bar,
                                height,
                            ),
                            color,
                            radius: [0.0; 4],
                            clip,
                        });
                    }
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
                    color: token_color(theme, "border", [0.24, 0.27, 0.36, 1.0]),
                    clip,
                });
            }
            UiWidgetSpec::Modal { open } if *open => {
                draw.commands.push(UiDrawCommand::Rect {
                    entity,
                    rect: UiRect::new(0.0, 0.0, 10_000.0, 10_000.0),
                    color: [0.0, 0.0, 0.0, 0.55],
                    radius: [0.0; 4],
                    clip,
                });
            }
            _ => {}
        }
    }
}

fn edit_text_style(world: &World, entity: Entity) -> UiTextStyle {
    world
        .get_component_store::<UiResolvedStyle>()
        .and_then(|store| store.get_component(entity.index).cloned())
        .and_then(|style| style.text)
        .map(|style| UiTextStyle {
            size: style.size,
            line_height: style.line_height,
            color: UiColor::Rgba(style.color),
            family: style.family,
            weight: style.weight,
            wrap: style.wrap,
            ellipsis: style.ellipsis,
        })
        .unwrap_or_default()
}

fn default_text(theme: Option<&UiThemes>) -> UiResolvedTextStyle {
    if let Some(themes) = theme
        && let Some(body) = themes.active.typography.get("body")
    {
        return themes.active.resolve_text(body);
    }
    UiResolvedTextStyle {
        size: 16.0,
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

fn prev_word_boundary(text: &str, cursor: usize) -> usize {
    let mut index = cursor.min(text.len());
    while index > 0 {
        let prev = prev_boundary(text, index);
        if text[prev..index].chars().all(char::is_whitespace) {
            index = prev;
        } else {
            break;
        }
    }
    while index > 0 {
        let prev = prev_boundary(text, index);
        if text[prev..index].chars().all(char::is_whitespace) {
            break;
        }
        index = prev;
    }
    index
}

fn next_word_boundary(text: &str, cursor: usize) -> usize {
    let mut index = cursor.min(text.len());
    while index < text.len() {
        let next = next_boundary(text, index);
        if text[index..next].chars().all(char::is_whitespace) {
            index = next;
        } else {
            break;
        }
    }
    while index < text.len() {
        let next = next_boundary(text, index);
        if text[index..next].chars().all(char::is_whitespace) {
            break;
        }
        index = next;
    }
    index
}

fn graph_palette(index: usize) -> [f32; 4] {
    const PALETTE: [[f32; 4]; 4] = [
        [0.50, 0.90, 0.60, 1.0],
        [0.40, 0.80, 1.00, 1.0],
        [0.45, 0.60, 1.00, 1.0],
        [1.00, 0.80, 0.40, 1.0],
    ];

    PALETTE[index % PALETTE.len()]
}

fn token_color(theme: Option<&UiThemes>, name: &str, fallback: [f32; 4]) -> [f32; 4] {
    theme
        .and_then(|themes| themes.active.color(name))
        .unwrap_or(fallback)
}

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}
