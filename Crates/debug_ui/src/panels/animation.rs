//! Panel for inspecting and driving imported animation players.

use std::fmt;

use iced_widget::{button, checkbox, pick_list, slider, text};
use iced_winit::core::{Color, Element, Length, Theme};

use ignit_ecs::{Entity, World};
use ignit_ecs_bridge::{AnimationClips, AnimationPlayer, SkeletonComponent, SkinningMode};

use crate::state::Renderer as IcedRenderer;
use crate::state::{DebugMessage, DebugUiState};

const ACCENT: Color = Color::from_rgb(0.7, 0.8, 1.0);
const MUTED: Color = Color::from_rgb(0.6, 0.6, 0.66);
/// Crossfade used when switching clips from the picker, in seconds.
const CROSSFADE_SECS: f32 = 0.25;
/// Upper bound on listed entities; each row costs a label lookup per rebuild.
const MAX_LISTED: usize = 32;

/// One selectable animated entity.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityChoice {
    entity: Entity,
    label: String,
}

impl fmt::Display for EntityChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Every entity with an [`AnimationPlayer`], in store order.
fn animated_entities(ecs: &World) -> Vec<EntityChoice> {
    let Some(players) = ecs.get_component_store::<AnimationPlayer>() else {
        return Vec::new();
    };

    ecs.entities()
        .filter(|entity| {
            players
                .sparse
                .get(entity.index)
                .copied()
                .flatten()
                .is_some()
        })
        .take(MAX_LISTED)
        .map(|entity| EntityChoice {
            entity,
            label: crate::panels::entity_label(ecs, &entity)
                .unwrap_or_else(|| format!("Entity #{}", entity.index)),
        })
        .collect()
}

/// Builds the animation panel.
pub fn animation_panel<'a>(ecs: &World) -> Element<'a, DebugMessage, Theme, IcedRenderer> {
    if ecs.get_resource::<DebugUiState>().is_none() {
        return crate::panels::blank();
    }

    let choices = animated_entities(ecs);
    if choices.is_empty() {
        return crate::panels::column_of(
            vec![text("No animated entities").size(12).color(MUTED).into()],
            6.0,
        )
        .into();
    }

    let selected = ecs
        .get_resource::<DebugUiState>()
        .and_then(|state| state.animation_entity)
        .filter(|entity| choices.iter().any(|choice| choice.entity == *entity))
        .unwrap_or(choices[0].entity);

    let Some(players) = ecs.get_component_store::<AnimationPlayer>() else {
        return crate::panels::blank();
    };
    let Some(player_index) = players.sparse.get(selected.index).copied().flatten() else {
        return crate::panels::blank();
    };
    let player = &players.components[player_index];

    let mut rows: Vec<Element<'static, DebugMessage, Theme, IcedRenderer>> = Vec::new();

    let picker = pick_list(
        choices
            .iter()
            .find(|choice| choice.entity == selected)
            .cloned(),
        choices,
        |choice: &EntityChoice| choice.to_string(),
    )
    .width(Length::Fill)
    .text_size(12)
    .on_select(|choice| DebugMessage::SelectAnimationEntity(Some(choice.entity)));
    rows.push(picker.into());

    rows.push(
        text(format!(
            "{}  ({:.2}s)",
            player.clip.name,
            player.clip.duration.max(0.0)
        ))
        .size(12)
        .color(ACCENT)
        .into(),
    );

    rows.push(crate::panels::caption("Skinning", ACCENT));
    rows.push(
        pick_list(
            Some(player.skinning),
            SkinningMode::ALL,
            |mode: &SkinningMode| mode.label().to_string(),
        )
        .width(Length::Fill)
        .text_size(12)
        .on_select(move |mode| DebugMessage::SetAnimationSkinning {
            entity: selected,
            mode,
        })
        .into(),
    );

    rows.push(
        iced_widget::row![
            button(text(if player.playing { "Pause" } else { "Play" }).size(12)).on_press(
                DebugMessage::SetAnimationPlaying {
                    entity: selected,
                    playing: !player.playing,
                }
            ),
            checkbox(player.looping)
                .label("Loop")
                .text_size(12)
                .on_toggle(move |looping| DebugMessage::SetAnimationLooping {
                    entity: selected,
                    looping,
                }),
        ]
        .spacing(8)
        .align_y(iced_winit::core::Alignment::Center)
        .into(),
    );

    rows.push(crate::panels::caption("Speed", ACCENT));
    rows.push(
        slider(0.1..=10.0, player.speed, move |speed| {
            DebugMessage::SetAnimationSpeed {
                entity: selected,
                speed,
            }
        })
        .into(),
    );

    rows.push(crate::panels::caption("Time", ACCENT));
    rows.push(
        slider(
            0.0..=player.clip.duration.max(0.001),
            player.time,
            move |time| DebugMessage::SetAnimationTime {
                entity: selected,
                time,
            },
        )
        .into(),
    );

    if let Some(blend) = &player.blend {
        rows.push(
            text(format!(
                "Fading out {} ({:.0}% left)",
                blend.clip.name,
                (1.0 - blend.progress).max(0.0) * 100.0
            ))
            .size(11)
            .color(MUTED)
            .into(),
        );
    }

    let clips = ecs
        .get_component_store::<AnimationClips>()
        .and_then(|clips| {
            let index = clips.sparse.get(selected.index).copied().flatten()?;
            let clips = &clips.components.get(index)?.0;
            (clips.len() > 1).then(|| clips.clone())
        });

    if let Some(clips) = clips {
        let names: Vec<String> = clips.iter().map(|clip| clip.name.clone()).collect();
        let current = names
            .iter()
            .find(|name| **name == player.clip.name)
            .cloned();
        rows.push(crate::panels::caption("Clips", ACCENT));
        rows.push(
            pick_list(current, names, |name: &String| name.clone())
                .width(Length::Fill)
                .text_size(12)
                .on_select(move |clip| DebugMessage::PlayAnimationClip {
                    entity: selected,
                    clip,
                })
                .into(),
        );
        rows.push(
            text(format!("Switches with a {CROSSFADE_SECS:.2}s crossfade."))
                .size(10)
                .color(MUTED)
                .into(),
        );
    }

    let joints = ecs
        .get_component_store::<SkeletonComponent>()
        .and_then(|skeletons| {
            let index = skeletons.sparse.get(selected.index).copied().flatten()?;
            let joints = skeletons.components[index].0.joints();
            (!joints.is_empty()).then(|| {
                joints
                    .iter()
                    .map(|joint| joint.name.clone())
                    .collect::<Vec<_>>()
            })
        });

    let weights_enabled = ecs
        .get_resource::<ignit_debug_render::DebugToggleState>()
        .is_some_and(|state| state.settings.weights);

    if weights_enabled && let Some(joints) = joints {
        let settings = ecs
            .get_resource::<ignit_debug_render::DebugToggleState>()
            .map(|state| state.settings);
        let current = settings
            .and_then(|settings| settings.weight_joint)
            .filter(|index| *index < joints.len())
            .map(|index| joints[index].clone())
            .or_else(|| joints.first().cloned());

        rows.push(crate::panels::caption("Skin weights", ACCENT));
        rows.push(
            pick_list(current, joints, |name: &String| name.clone())
                .width(Length::Fill)
                .text_size(12)
                .on_select(move |joint| DebugMessage::SetAnimationWeightJoint {
                    entity: selected,
                    joint,
                })
                .into(),
        );
    }

    crate::panels::column_of(rows, 6.0).into()
}
