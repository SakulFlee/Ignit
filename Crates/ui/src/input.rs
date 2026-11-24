//! UI input state, hit testing, and pointer routing.

use ignit_ecs::{Entity, Events, World};

use crate::events::UiEvent;
use crate::focus;
use crate::geometry::UiPoint;
use crate::id::UiId;
use crate::node::{UiAction, UiComputed, UiInteraction, UiNode, UiScroll};
use crate::style::{UiCursorIcon, UiResolvedStyle};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKey {
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Space,
    Character(char),
}

/// Aggregated pointer/keyboard state for one frame.
#[derive(Clone, Debug)]
pub struct UiInputState {
    pub pointer: UiPoint,
    pub pointer_delta: UiPoint,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub wheel: UiPoint,
    pub keys_pressed: Vec<UiKey>,
    pub text: String,
    pub modifiers: UiModifiers,
    pub pressed_entity: Option<Entity>,
}

impl Default for UiInputState {
    fn default() -> Self {
        Self {
            pointer: UiPoint::ZERO,
            pointer_delta: UiPoint::ZERO,
            primary_down: false,
            primary_pressed: false,
            primary_released: false,
            wheel: UiPoint::ZERO,
            keys_pressed: Vec::new(),
            text: String::new(),
            modifiers: UiModifiers::default(),
            pressed_entity: None,
        }
    }
}

impl UiInputState {
    /// Clears one-frame edges after routing. Persistent state (pointer,
    /// button-down, modifiers) is kept for the next frame.
    pub fn end_frame(&mut self) {
        self.pointer_delta = UiPoint::ZERO;
        self.primary_pressed = false;
        self.primary_released = false;
        self.wheel = UiPoint::ZERO;
        self.keys_pressed.clear();
        self.text.clear();
    }

    pub fn key_pressed(&self, key: UiKey) -> bool {
        self.keys_pressed.contains(&key)
    }
}

/// What the UI consumed this frame. The game input snapshot is masked from it.
#[derive(Clone, Debug, Default)]
pub struct UiInputCapture {
    pub pointer: bool,
    pub wheel: bool,
    pub keyboard: bool,
    pub text: bool,
    pub cursor: Option<UiCursorIcon>,
}

impl UiInputCapture {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug)]
pub struct UiHit {
    pub entity: Entity,
    pub id: UiId,
    pub rect: crate::geometry::UiRect,
}

/// Topmost enabled UI node at a point, honoring clips and z order.
pub fn hit_test(world: &World, point: UiPoint) -> Option<UiHit> {
    let computed = world.get_component_store::<UiComputed>()?;
    let nodes = world.get_component_store::<UiNode>()?;
    let interaction = world.get_component_store::<UiInteraction>();

    let mut best: Option<(i32, usize, UiHit)> = None;
    for entity in world.entities() {
        let Some(rect) = computed.get_component(entity.index) else {
            continue;
        };
        if !rect.is_hit(point) {
            continue;
        }
        if interaction
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
            .is_some_and(|state| state.disabled)
        {
            continue;
        }
        let Some(node) = nodes.get_component(entity.index) else {
            continue;
        };
        let hit = UiHit {
            entity,
            id: node.id.clone(),
            rect: rect.rect,
        };
        let key = (rect.z_index, entity.index);
        if best.as_ref().is_none_or(|(z, index, _)| key > (*z, *index)) {
            best = Some((key.0, key.1, hit));
        }
    }
    best.map(|(_, _, hit)| hit)
}

/// Routes pointer/click/scroll input and emits UI events.
pub fn process_pointer(
    world: &mut World,
    input: &mut UiInputState,
    capture: &mut UiInputCapture,
) -> Option<UiHit> {
    let hit = hit_test(world, input.pointer);
    let hit_entity = hit.as_ref().map(|hit| hit.entity);

    {
        let mut interactions = world.get_component_store_mut::<UiInteraction>();
        if let Some(store) = interactions.as_mut() {
            let store = store.get_mut_store();
            for entity in world.entities() {
                if let Some(interaction) = store.get_component_mut(entity.index) {
                    interaction.hovered = Some(entity) == hit_entity;
                    interaction.pointer_inside = Some(entity) == hit_entity;
                    interaction.pressed = Some(entity) == hit_entity && input.primary_down;
                }
            }
        }
    }

    if let Some(hit) = &hit {
        if input.primary_pressed {
            if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
                events.send(UiEvent::Press {
                    entity: hit.entity,
                    id: hit.id.clone(),
                });
            }
            input.pressed_entity = Some(hit.entity);
            focus::focus_entity(world, hit.entity);
        }
        if input.primary_released {
            let action = world
                .get_component_store::<UiAction>()
                .and_then(|store| store.get_component(hit.entity.index).cloned());
            if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
                events.send(UiEvent::Release {
                    entity: hit.entity,
                    id: hit.id.clone(),
                });
                if input.pressed_entity == Some(hit.entity) {
                    events.send(UiEvent::Click {
                        entity: hit.entity,
                        id: hit.id.clone(),
                    });
                    if let Some(action) = action {
                        events.send(UiEvent::Action {
                            entity: hit.entity,
                            id: hit.id.clone(),
                            action,
                        });
                    }
                }
            }
            input.pressed_entity = None;
        }

        if input.wheel != UiPoint::ZERO
            && let Some(mut scroll) = world
                .get_component_store_mut::<UiScroll>()
                .and_then(|store| store.get_component(hit.entity.index).copied())
        {
            scroll.offset.x -= input.wheel.x;
            scroll.offset.y -= input.wheel.y;
            scroll.clamp();
            if let Some(store) = world.get_component_store_mut::<UiScroll>().as_mut() {
                store.get_mut_store().attach(hit.entity.index, scroll);
            }
            if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
                events.send(UiEvent::Scroll {
                    entity: hit.entity,
                    id: hit.id.clone(),
                    delta: input.wheel,
                });
            }
        }

        let cursor = world
            .get_component_store::<UiResolvedStyle>()
            .and_then(|store| {
                store
                    .get_component(hit.entity.index)
                    .map(|style| style.cursor)
            })
            .unwrap_or_default();
        capture.pointer = true;
        capture.wheel = input.wheel != UiPoint::ZERO;
        capture.cursor = Some(cursor);
    } else if input.primary_released {
        input.pressed_entity = None;
    }

    hit
}
