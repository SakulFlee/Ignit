//! UI events emitted to game systems.

use ignit_ecs::Entity;

use crate::geometry::UiPoint;
use crate::id::UiId;
use crate::node::UiAction;

#[derive(Clone, Debug)]
pub enum UiEvent {
    Hover {
        entity: Entity,
        id: UiId,
    },
    Click {
        entity: Entity,
        id: UiId,
    },
    Press {
        entity: Entity,
        id: UiId,
    },
    Release {
        entity: Entity,
        id: UiId,
    },
    Focus {
        entity: Entity,
        id: UiId,
    },
    Blur {
        entity: Entity,
        id: UiId,
    },
    Submit {
        entity: Entity,
        id: UiId,
        value: String,
    },
    ValueChanged {
        entity: Entity,
        id: UiId,
        value: String,
    },
    Drag {
        entity: Entity,
        id: UiId,
        delta: UiPoint,
    },
    Scroll {
        entity: Entity,
        id: UiId,
        delta: UiPoint,
    },
    Action {
        entity: Entity,
        id: UiId,
        action: UiAction,
    },
}

impl UiEvent {
    pub fn entity(&self) -> Entity {
        match self {
            Self::Hover { entity, .. }
            | Self::Click { entity, .. }
            | Self::Press { entity, .. }
            | Self::Release { entity, .. }
            | Self::Focus { entity, .. }
            | Self::Blur { entity, .. }
            | Self::Submit { entity, .. }
            | Self::ValueChanged { entity, .. }
            | Self::Drag { entity, .. }
            | Self::Scroll { entity, .. }
            | Self::Action { entity, .. } => *entity,
        }
    }

    pub fn id(&self) -> &UiId {
        match self {
            Self::Hover { id, .. }
            | Self::Click { id, .. }
            | Self::Press { id, .. }
            | Self::Release { id, .. }
            | Self::Focus { id, .. }
            | Self::Blur { id, .. }
            | Self::Submit { id, .. }
            | Self::ValueChanged { id, .. }
            | Self::Drag { id, .. }
            | Self::Scroll { id, .. }
            | Self::Action { id, .. } => id,
        }
    }
}
