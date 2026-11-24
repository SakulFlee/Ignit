//! Renderer-agnostic draw list built from resolved ECS state.

use ignit_ecs::{Entity, World};

use crate::geometry::{UiPoint, UiRect};
use crate::node::{UiComputed, UiNode};
use crate::style::{UiImage, UiResolvedStyle, UiResolvedTextStyle, UiText};
use crate::widgets::{UiWidget, UiWidgetSpec};

#[derive(Clone, Debug)]
pub enum UiDrawCommand {
    Shadow {
        entity: Entity,
        rect: UiRect,
        color: [f32; 4],
        offset: UiPoint,
        blur: f32,
        spread: f32,
        radius: [f32; 4],
        clip: Option<UiRect>,
    },
    Rect {
        entity: Entity,
        rect: UiRect,
        color: [f32; 4],
        radius: [f32; 4],
        clip: Option<UiRect>,
    },
    Border {
        entity: Entity,
        rect: UiRect,
        width: f32,
        color: [f32; 4],
        radius: [f32; 4],
        clip: Option<UiRect>,
    },
    Text {
        entity: Entity,
        rect: UiRect,
        text: String,
        style: UiResolvedTextStyle,
        clip: Option<UiRect>,
    },
    Image {
        entity: Entity,
        rect: UiRect,
        texture: String,
        tint: [f32; 4],
        uv: [f32; 4],
        nine_slice: Option<[f32; 4]>,
        clip: Option<UiRect>,
    },
    Line {
        entity: Entity,
        rect: UiRect,
        color: [f32; 4],
        clip: Option<UiRect>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct UiDrawList {
    pub commands: Vec<UiDrawCommand>,
}

impl UiDrawList {
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }
}

/// Builds a sorted draw list. Commands are ordered by the layout z index, so
/// parents paint before children and later roots paint on top.
pub fn build_draw_list(world: &World, list: &mut UiDrawList) {
    list.commands.clear();
    let Some(computed) = world.get_component_store::<UiComputed>() else {
        return;
    };
    let nodes = world.get_component_store::<UiNode>();
    let styles = world.get_component_store::<UiResolvedStyle>();
    let texts = world.get_component_store::<UiText>();
    let images = world.get_component_store::<UiImage>();
    let widgets = world.get_component_store::<UiWidget>();

    let mut entities: Vec<(i32, usize, Entity)> = world
        .entities()
        .filter_map(|entity| {
            let computed = computed.get_component(entity.index)?;
            computed
                .visible
                .then_some((computed.z_index, entity.index, entity))
        })
        .collect();
    entities.sort_unstable_by_key(|(z, index, _)| (*z, *index));

    for (_, _, entity) in entities {
        let Some(computed) = computed.get_component(entity.index) else {
            continue;
        };
        let Some(style) = styles
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
        else {
            continue;
        };
        let clip = computed.clip;

        if let Some(shadow) = &style.shadow
            && (shadow.blur > 0.0 || shadow.spread > 0.0)
        {
            list.commands.push(UiDrawCommand::Shadow {
                entity,
                rect: computed.rect,
                color: shadow.color,
                offset: shadow.offset,
                blur: shadow.blur,
                spread: shadow.spread,
                radius: corners(style.radius),
                clip,
            });
        }
        if let Some(color) = style.background {
            list.commands.push(UiDrawCommand::Rect {
                entity,
                rect: computed.rect,
                color: apply_opacity(color, style.opacity),
                radius: corners(style.radius),
                clip,
            });
        }
        if let Some(border) = &style.border {
            list.commands.push(UiDrawCommand::Border {
                entity,
                rect: computed.rect,
                width: border.width,
                color: apply_opacity(border.color, style.opacity),
                radius: corners(style.radius),
                clip,
            });
        }
        if let Some(text) = texts
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
            && let Some(style) = &style.text
        {
            let text_clip = Some(match clip {
                Some(clip) => clip.intersect(computed.content).unwrap_or(UiRect::ZERO),
                None => computed.content,
            });
            if text_clip.is_some_and(|clip| clip.is_empty()) {
                continue;
            }
            let mut text_rect = computed.content;
            if let Some(widget) = widgets
                .as_ref()
                .and_then(|store| store.get_component(entity.index))
            {
                match &widget.spec {
                    UiWidgetSpec::TextBox { .. } => {
                        text_rect.x -= widget.scroll_x;
                        let line_height = style.line_height.unwrap_or(style.size * 1.25);
                        text_rect.y += ((text_rect.height - line_height) * 0.5).max(0.0);
                    }
                    UiWidgetSpec::Button { .. } | UiWidgetSpec::Collapsible { .. } => {
                        let line_height = style.line_height.unwrap_or(style.size * 1.25);
                        text_rect.y += ((text_rect.height - line_height) * 0.5).max(0.0);
                    }
                    _ => {}
                }
            }
            list.commands.push(UiDrawCommand::Text {
                entity,
                rect: text_rect,
                text: text.value.clone(),
                style: style.clone(),
                clip: text_clip,
            });
        }
        if let Some(image) = images
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
            && let Some(style) = &style.image
        {
            list.commands.push(UiDrawCommand::Image {
                entity,
                rect: computed.rect,
                texture: image.texture.clone(),
                tint: apply_opacity(style.tint, style_opacity(world, entity)),
                uv: style.uv,
                nine_slice: style.nine_slice,
                clip,
            });
        }

        // Nodes with children but no own content still benefit from a debug
        // outline when selected; selection belongs to the editor, so nothing
        // extra is emitted here.
        let _ = nodes;
    }
}

fn style_opacity(world: &World, entity: Entity) -> f32 {
    world
        .get_component_store::<UiResolvedStyle>()
        .and_then(|store| store.get_component(entity.index).map(|style| style.opacity))
        .unwrap_or(1.0)
}

fn apply_opacity(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] *= opacity.clamp(0.0, 1.0);
    color
}

fn corners(corners: crate::style::UiCorners) -> [f32; 4] {
    [
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ]
}

#[derive(Clone, Debug, Default)]
pub struct UiStats {
    pub nodes: usize,
    pub roots: usize,
    pub draw_commands: usize,
    pub layout_ms: f32,
    pub style_ms: f32,
}
