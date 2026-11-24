//! Focus state, tab order, arrow navigation, and modal scopes.

use ignit_ecs::{Entity, Events, World};

use crate::events::UiEvent;
use crate::geometry::UiRect;
use crate::node::{UiComputed, UiFocusable, UiInteraction, UiNode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiDirection {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, Default)]
pub struct UiFocusState {
    pub focused: Option<Entity>,
    pub focus_visible: bool,
    pub modal: Option<Entity>,
}

impl UiFocusState {
    pub fn is_focused(&self, entity: Entity) -> bool {
        self.focused == Some(entity)
    }
}

pub fn focus_entity(world: &mut World, entity: Entity) {
    if let Some(modal) = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.modal)
        && !is_descendant(world, entity, modal)
    {
        return;
    }
    let previous = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);
    if previous == Some(entity) {
        return;
    }

    if let Some(previous) = previous {
        set_interaction_focus(world, previous, false);
        if let Some(id) = node_id(world, previous)
            && let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>()
        {
            events.send(UiEvent::Blur {
                entity: previous,
                id,
            });
        }
    }

    if let Some(mut state) = world.get_resource_mut::<UiFocusState>() {
        state.focused = Some(entity);
        state.focus_visible = true;
    }
    set_interaction_focus(world, entity, true);
    if let Some(id) = node_id(world, entity)
        && let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>()
    {
        events.send(UiEvent::Focus { entity, id });
    }
}

pub fn blur(world: &mut World) {
    let previous = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);
    let Some(previous) = previous else {
        return;
    };
    set_interaction_focus(world, previous, false);
    if let Some(mut state) = world.get_resource_mut::<UiFocusState>() {
        state.focused = None;
        state.focus_visible = false;
    }
    if let Some(id) = node_id(world, previous)
        && let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>()
    {
        events.send(UiEvent::Blur {
            entity: previous,
            id,
        });
    }
}

pub fn tab_order(world: &World) -> Vec<Entity> {
    let Some(focusable) = world.get_component_store::<UiFocusable>() else {
        return Vec::new();
    };
    let Some(computed) = world.get_component_store::<UiComputed>() else {
        return Vec::new();
    };
    let modal = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.modal);
    let mut entries: Vec<(i32, usize)> = Vec::new();
    for entity in world.entities() {
        let Some(focus) = focusable.get_component(entity.index) else {
            continue;
        };
        if focus.disabled {
            continue;
        }
        if computed
            .get_component(entity.index)
            .is_some_and(|computed| !computed.visible)
        {
            continue;
        }
        if let Some(modal) = modal
            && !is_descendant(world, entity, modal)
        {
            continue;
        }
        entries.push((focus.tab_index, entity.index));
    }
    entries.sort_unstable();
    entries
        .into_iter()
        .map(|(_, index)| {
            let generation = world.generation(index);
            Entity::new(index, generation)
        })
        .collect()
}

pub fn focus_next(world: &mut World, backwards: bool) {
    let order = tab_order(world);
    if order.is_empty() {
        blur(world);
        return;
    }
    let current = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);
    let index = current.and_then(|entity| order.iter().position(|candidate| *candidate == entity));
    let next = match (index, backwards) {
        (Some(index), true) => order[(index + order.len() - 1) % order.len()],
        (Some(index), false) => order[(index + 1) % order.len()],
        (None, true) => *order.last().unwrap(),
        (None, false) => order[0],
    };
    focus_entity(world, next);
}

pub fn focus_direction(world: &mut World, direction: UiDirection) {
    let current = world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.focused);
    let Some(current_entity) = current else {
        focus_next(world, false);
        return;
    };
    let Some(current) = world
        .get_component_store::<UiComputed>()
        .and_then(|store| store.get_component(current_entity.index).copied())
    else {
        return;
    };
    let origin = center(current.rect);
    let order = tab_order(world);
    let mut best: Option<(f32, Entity)> = None;
    for candidate in order {
        if candidate == current_entity {
            continue;
        }
        let Some(rect) = world
            .get_component_store::<UiComputed>()
            .and_then(|store| store.get_component(candidate.index).copied())
        else {
            continue;
        };
        let point = center(rect.rect);
        let dx = point.0 - origin.0;
        let dy = point.1 - origin.1;
        let (primary, secondary) = match direction {
            UiDirection::Left if dx < -0.5 => (-dx, dy.abs()),
            UiDirection::Right if dx > 0.5 => (dx, dy.abs()),
            UiDirection::Up if dy < -0.5 => (-dy, dx.abs()),
            UiDirection::Down if dy > 0.5 => (dy, dx.abs()),
            _ => continue,
        };
        let score = primary + secondary * 2.0;
        if best.is_none_or(|(best_score, _)| score < best_score) {
            best = Some((score, candidate));
        }
    }
    if let Some((_, entity)) = best {
        focus_entity(world, entity);
    }
}

pub fn set_modal(world: &mut World, root: Option<Entity>) {
    if let Some(mut state) = world.get_resource_mut::<UiFocusState>() {
        state.modal = root;
    }
}

pub fn modal_root(world: &World) -> Option<Entity> {
    world
        .get_resource::<UiFocusState>()
        .and_then(|state| state.modal)
}

fn center(rect: UiRect) -> (f32, f32) {
    (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

fn set_interaction_focus(world: &mut World, entity: Entity, focused: bool) {
    if let Some(store) = world.get_component_store_mut::<UiInteraction>()
        && let Some(interaction) = store.get_mut_store().get_component_mut(entity.index)
    {
        interaction.focused = focused;
    }
}

fn node_id(world: &World, entity: Entity) -> Option<crate::id::UiId> {
    world.get_component_store::<UiNode>().and_then(|store| {
        store
            .get_component(entity.index)
            .map(|node| node.id.clone())
    })
}

fn is_descendant(world: &World, entity: Entity, root: Entity) -> bool {
    if entity == root {
        return true;
    }
    let nodes = world.get_component_store::<UiNode>();
    let mut current = nodes
        .as_ref()
        .and_then(|store| store.get_component(entity.index))
        .and_then(|node| node.parent);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = nodes
            .as_ref()
            .and_then(|store| store.get_component(candidate.index))
            .and_then(|node| node.parent);
    }
    false
}
