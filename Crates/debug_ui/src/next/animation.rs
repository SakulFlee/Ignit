//! New-UI animation debug panel (first slice).

use ignit_ecs::{Entity, World};
use ignit_ecs_bridge::{AnimationClips, AnimationPlayer};
use ignit_ui::{
    UiAction, UiColor, UiDimension, UiDocumentNode, UiEvent, UiLayout, UiStyle, UiTextStyle,
    UiWidget, UiWidgetSpec,
};

use crate::state::{DebugMessage, DebugUiState, PanelId};

const WINDOW_ID: &str = "debug_anim_window";
const ENTITY_PREFIX: &str = "debug_anim_entity_";
const CLIP_PREFIX: &str = "debug_anim_clip_";
const ROWS: usize = 12;
const CLIP_ROWS: usize = 8;

pub(super) fn build() -> UiDocumentNode {
    let mut left = UiDocumentNode::new("debug_anim_entities").layout(
        UiLayout::column()
            .width(190.0)
            .height_full()
            .gap(2.0)
            .shrink(0.0),
    );
    for index in 0..ROWS {
        left = left.child(
            UiDocumentNode::new(format!("{ENTITY_PREFIX}{index}"))
                .layout(UiLayout::default().width_full().height(20.0).shrink(0.0))
                .widget(UiWidgetSpec::Button {
                    label: String::new(),
                })
                .action(UiAction::new(format!("{ENTITY_PREFIX}{index}")))
                .focusable()
                .style(row_style(false, false)),
        );
    }

    let mut clips =
        UiDocumentNode::new("debug_anim_clips").layout(UiLayout::column().width_full().gap(2.0));
    for index in 0..CLIP_ROWS {
        clips = clips.child(
            UiDocumentNode::new(format!("{CLIP_PREFIX}{index}"))
                .layout(UiLayout::default().width_full().height(20.0).shrink(0.0))
                .widget(UiWidgetSpec::Button {
                    label: String::new(),
                })
                .action(UiAction::new(format!("{CLIP_PREFIX}{index}")))
                .focusable()
                .style(row_style(false, false)),
        );
    }

    let right = UiDocumentNode::new("debug_anim_right")
        .layout(UiLayout {
            flex_grow: 1.0,
            ..UiLayout::column().height_full().gap(4.0)
        })
        .child(
            UiDocumentNode::new("debug_anim_clip_label")
                .text("No animated entity")
                .style(text_style()),
        )
        .child(
            UiDocumentNode::new("debug_anim_buttons")
                .layout(
                    UiLayout::row()
                        .width_full()
                        .height(24.0)
                        .gap(6.0)
                        .shrink(0.0),
                )
                .child(
                    UiDocumentNode::new("debug_anim_play")
                        .layout(
                            UiLayout::row()
                                .size(64.0, 22.0)
                                .justify(ignit_ui::UiJustify::Center)
                                .align_items(ignit_ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Pause".into(),
                        })
                        .action(UiAction::new("debug_anim_play"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("debug_anim_play_label").text("Pause")),
                )
                .child(
                    UiDocumentNode::new("debug_anim_loop_row")
                        .layout(UiLayout::row().height(22.0).gap(4.0))
                        .child(UiDocumentNode::new("debug_anim_loop_label").text("Loop"))
                        .child(
                            UiDocumentNode::new("debug_anim_loop")
                                .layout(UiLayout::default().size(40.0, 22.0))
                                .widget(UiWidgetSpec::Toggle { on: true }),
                        ),
                ),
        )
        .child(
            UiDocumentNode::new("debug_anim_speed_label")
                .text("Speed 1.00")
                .style(text_style()),
        )
        .child(
            UiDocumentNode::new("debug_anim_speed")
                .layout(UiLayout::default().width_full().height(22.0))
                .widget(UiWidgetSpec::Slider {
                    value: 1.0,
                    min: 0.1,
                    max: 10.0,
                })
                .focusable(),
        )
        .child(
            UiDocumentNode::new("debug_anim_time_label")
                .text("Time 0.00s")
                .style(text_style()),
        )
        .child(
            UiDocumentNode::new("debug_anim_time")
                .layout(UiLayout::default().width_full().height(22.0))
                .widget(UiWidgetSpec::Slider {
                    value: 0.0,
                    min: 0.0,
                    max: 1.0,
                })
                .focusable(),
        )
        .child(
            UiDocumentNode::new("debug_anim_blend")
                .text("")
                .style(text_style()),
        )
        .child(
            UiDocumentNode::new("debug_anim_clip_heading")
                .text("Clips")
                .style(text_style()),
        )
        .child(clips);

    let body = UiDocumentNode::new("debug_anim_body")
        .layout(
            UiLayout::row()
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: UiDimension::Length(8.0),
                    right: UiDimension::Length(8.0),
                    top: UiDimension::Length(34.0),
                    bottom: UiDimension::Length(8.0),
                })
                .gap(8.0),
        )
        .child(left)
        .child(right);

    UiDocumentNode::new(WINDOW_ID)
        .layout(
            UiLayout::column()
                .width(460.0)
                .height(430.0)
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: UiDimension::Length(520.0),
                    right: UiDimension::Auto,
                    top: UiDimension::Length(260.0),
                    bottom: UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "Animation".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel"))
        .child(body)
        .child(super::close_button("debug_anim_close", "debug_anim_hide"))
}

fn text_style() -> UiStyle {
    UiStyle::default().padding_text(UiTextStyle {
        size: 11.0,
        wrap: ignit_ui::UiTextWrap::NoWrap,
        ..Default::default()
    })
}

fn row_style(visible: bool, selected: bool) -> UiStyle {
    UiStyle::default()
        .radius(4.0)
        .padding_text(UiTextStyle {
            size: 11.0,
            color: if selected {
                UiColor::token("accent")
            } else {
                UiColor::token("text")
            },
            wrap: ignit_ui::UiTextWrap::NoWrap,
            ..Default::default()
        })
        .visible(visible)
}

fn animated_entities(world: &World) -> Vec<Entity> {
    let Some(players) = world.get_component_store::<AnimationPlayer>() else {
        return Vec::new();
    };
    world
        .entities()
        .filter(|entity| {
            players
                .sparse
                .get(entity.index)
                .copied()
                .flatten()
                .is_some()
        })
        .take(ROWS)
        .collect()
}

fn selected_entity(world: &World) -> Option<Entity> {
    let choices = animated_entities(world);
    world
        .get_resource::<DebugUiState>()
        .and_then(|state| state.animation_entity)
        .filter(|entity| choices.contains(entity))
        .or_else(|| choices.first().copied())
}

pub(super) fn refresh(world: &mut World) {
    let entities = animated_entities(world);
    let selected = selected_entity(world);
    for index in 0..ROWS {
        let id = format!("{ENTITY_PREFIX}{index}");
        if let Some(entity) = entities.get(index) {
            let label = crate::panels::entity_label(world, entity)
                .unwrap_or_else(|| format!("Entity #{}", entity.index));
            super::set_text(world, &id, format!("#{} {label}", entity.index));
            set_row_style(world, &id, true, Some(*entity) == selected);
        } else {
            set_row_style(world, &id, false, false);
        }
    }

    let Some(selected) = selected else {
        super::set_text(world, "debug_anim_clip_label", "No animated entity".into());
        for index in 0..CLIP_ROWS {
            set_row_style(world, &format!("{CLIP_PREFIX}{index}"), false, false);
        }
        return;
    };

    let player = world
        .get_component_store::<AnimationPlayer>()
        .and_then(|store| {
            store
                .sparse
                .get(selected.index)
                .copied()
                .flatten()
                .map(|index| store.components[index].clone())
        });
    let Some(player) = player else {
        return;
    };

    super::set_text(
        world,
        "debug_anim_clip_label",
        format!(
            "{}  ({:.2}s)",
            player.clip.name,
            player.clip.duration.max(0.0)
        ),
    );
    super::set_text(
        world,
        "debug_anim_play_label",
        if player.playing {
            "Pause".into()
        } else {
            "Play".into()
        },
    );
    set_toggle(world, "debug_anim_loop", player.looping);
    super::set_text(
        world,
        "debug_anim_speed_label",
        format!("Speed {:.2}", player.speed),
    );
    set_slider(world, "debug_anim_speed", 0.1, 10.0, player.speed);
    let duration = player.clip.duration.max(0.001);
    super::set_text(
        world,
        "debug_anim_time_label",
        format!("Time {:.2}s / {:.2}s", player.time, duration),
    );
    set_slider(world, "debug_anim_time", 0.0, duration, player.time);
    super::set_text(
        world,
        "debug_anim_blend",
        player.blend.as_ref().map_or_else(String::new, |blend| {
            format!(
                "Fading out {} ({:.0}% left)",
                blend.clip.name,
                (1.0 - blend.progress).max(0.0) * 100.0
            )
        }),
    );

    let clips = world
        .get_component_store::<AnimationClips>()
        .and_then(|store| {
            store
                .sparse
                .get(selected.index)
                .copied()
                .flatten()
                .and_then(|index| store.components.get(index))
                .map(|clips| clips.0.clone())
        })
        .unwrap_or_default();
    for index in 0..CLIP_ROWS {
        let id = format!("{CLIP_PREFIX}{index}");
        if let Some(clip) = clips.get(index) {
            super::set_text(world, &id, clip.name.clone());
            set_row_style(world, &id, true, clip.name == player.clip.name);
        } else {
            set_row_style(world, &id, false, false);
        }
    }
    super::set_text(
        world,
        "debug_anim_clip_heading",
        format!("Clips ({})", clips.len()),
    );

    let open = world
        .get_resource::<DebugUiState>()
        .map(|state| state.is_panel_visible(PanelId::Animation))
        .unwrap_or(false);
    if let Some(entity) = find(world, WINDOW_ID) {
        super::set_panel_open(world, entity, open);
    }
}

pub(super) fn handle_event(world: &mut World, event: &UiEvent) -> bool {
    match event {
        UiEvent::Action { action, .. } if action.name.starts_with(ENTITY_PREFIX) => {
            if let Some(index) = parse_index(&action.name, ENTITY_PREFIX)
                && let Some(entity) = animated_entities(world).get(index).copied()
            {
                crate::handle_debug_message_world(
                    DebugMessage::SelectAnimationEntity(Some(entity)),
                    world,
                );
                return true;
            }
            false
        }
        UiEvent::Action { action, .. } if action.name == "debug_anim_play" => {
            if let Some(entity) = selected_entity(world)
                && let Some(playing) =
                    world
                        .get_component_store::<AnimationPlayer>()
                        .and_then(|store| {
                            store
                                .sparse
                                .get(entity.index)
                                .copied()
                                .flatten()
                                .map(|index| store.components[index].playing)
                        })
            {
                crate::handle_debug_message_world(
                    DebugMessage::SetAnimationPlaying {
                        entity,
                        playing: !playing,
                    },
                    world,
                );
                return true;
            }
            false
        }
        UiEvent::Action { action, .. } if action.name.starts_with(CLIP_PREFIX) => {
            if let Some(index) = parse_index(&action.name, CLIP_PREFIX)
                && let Some(entity) = selected_entity(world)
                && let Some(name) = clip_name(world, entity, index)
            {
                crate::handle_debug_message_world(
                    DebugMessage::PlayAnimationClip { entity, clip: name },
                    world,
                );
                return true;
            }
            false
        }
        UiEvent::Action { action, .. } if action.name == "debug_anim_hide" => {
            crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Animation), world);
            true
        }
        UiEvent::ValueChanged { id, value, .. } => {
            let Some(entity) = selected_entity(world) else {
                return false;
            };
            match id.as_str() {
                "debug_anim_loop" => {
                    if let Ok(looping) = value.parse::<bool>() {
                        crate::handle_debug_message_world(
                            DebugMessage::SetAnimationLooping { entity, looping },
                            world,
                        );
                        return true;
                    }
                }
                "debug_anim_speed" => {
                    if let Ok(speed) = value.parse::<f32>() {
                        crate::handle_debug_message_world(
                            DebugMessage::SetAnimationSpeed { entity, speed },
                            world,
                        );
                        return true;
                    }
                }
                "debug_anim_time" => {
                    if let Ok(time) = value.parse::<f32>() {
                        crate::handle_debug_message_world(
                            DebugMessage::SetAnimationTime { entity, time },
                            world,
                        );
                        return true;
                    }
                }
                _ => {}
            }
            false
        }
        _ => false,
    }
}

fn clip_name(world: &World, entity: Entity, index: usize) -> Option<String> {
    world
        .get_component_store::<AnimationClips>()
        .and_then(|store| {
            store
                .sparse
                .get(entity.index)
                .copied()
                .flatten()
                .and_then(|row| store.components.get(row))
                .and_then(|clips| clips.0.get(index))
                .map(|clip| clip.name.clone())
        })
}

fn parse_index(action: &str, prefix: &str) -> Option<usize> {
    action.strip_prefix(prefix)?.parse().ok()
}

fn set_row_style(world: &mut World, id: &str, visible: bool, selected: bool) {
    let Some(entity) = find(world, id) else {
        return;
    };
    let style = row_style(visible, selected);
    let current = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned());
    if current.as_ref() != Some(&style) {
        let _ = world.attach_component(&entity, style);
        super::mark_dirty(
            world,
            entity,
            ignit_ui::UiDirty::LAYOUT | ignit_ui::UiDirty::PAINT,
        );
    }
}

fn set_toggle(world: &mut World, id: &str, on: bool) {
    let Some(entity) = find(world, id) else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Toggle { on: current } = &mut widget.spec
        && *current != on
    {
        *current = on;
        changed = true;
    }
    if changed {
        super::mark_dirty(world, entity, ignit_ui::UiDirty::PAINT);
    }
}

fn set_slider(world: &mut World, id: &str, min: f32, max: f32, value: f32) {
    let Some(entity) = find(world, id) else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Slider {
            value: current,
            min: current_min,
            max: current_max,
        } = &mut widget.spec
        && ((*current - value).abs() > 1.0e-4 || *current_min != min || *current_max != max)
    {
        *current = value;
        *current_min = min;
        *current_max = max;
        changed = true;
    }
    if changed {
        super::mark_dirty(world, entity, ignit_ui::UiDirty::PAINT);
    }
}

fn find(world: &World, id: &str) -> Option<Entity> {
    world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
}
