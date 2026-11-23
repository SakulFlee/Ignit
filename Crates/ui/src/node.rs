//! ECS components for retained UI nodes.

use bitflags::bitflags;
use ignit_ecs::Entity;
use serde::{Deserialize, Serialize};

use crate::geometry::{UiPoint, UiRect, UiSize};
use crate::id::UiId;
use crate::style::UiStyle;

/// Hierarchy data for one UI node.
#[derive(Clone, Debug)]
pub struct UiNode {
    pub id: UiId,
    pub parent: Option<Entity>,
    pub children: Vec<Entity>,
}

impl UiNode {
    pub fn new(id: impl Into<UiId>) -> Self {
        Self {
            id: id.into(),
            parent: None,
            children: Vec::new(),
        }
    }
}

/// Root viewport and ordering for a UI tree.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct UiView {
    pub size: UiSize,
    pub scale: f32,
    pub order: i32,
    pub visible: bool,
    pub modal: bool,
}

impl Default for UiView {
    fn default() -> Self {
        Self {
            size: UiSize::new(800.0, 600.0),
            scale: 1.0,
            order: 0,
            visible: true,
            modal: false,
        }
    }
}

impl UiView {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            size: UiSize::new(width, height),
            ..Default::default()
        }
    }
}

/// Theme selection and inherited classes for a subtree.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiThemeScope {
    pub theme: Option<String>,
    pub classes: Vec<String>,
}

impl UiThemeScope {
    pub fn theme(theme: impl Into<String>) -> Self {
        Self {
            theme: Some(theme.into()),
            classes: Vec::new(),
        }
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

/// Runtime interaction state.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct UiInteraction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    pub pointer_inside: bool,
    pub captured: bool,
}

impl UiInteraction {
    pub fn state(&self) -> crate::style::UiState {
        if self.disabled {
            crate::style::UiState::Disabled
        } else if self.pressed {
            crate::style::UiState::Pressed
        } else if self.hovered {
            crate::style::UiState::Hovered
        } else if self.focused {
            crate::style::UiState::Focused
        } else {
            crate::style::UiState::Normal
        }
    }
}

/// Keyboard focus order and ring behavior.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct UiFocusable {
    pub tab_index: i32,
    pub focus_ring: bool,
    pub disabled: bool,
}

impl Default for UiFocusable {
    fn default() -> Self {
        Self {
            tab_index: 0,
            focus_ring: true,
            disabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiActionKind {
    #[default]
    Click,
    Submit,
    ValueChanged,
    Custom,
}

/// Semantic action emitted when a widget is activated.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiAction {
    pub kind: UiActionKind,
    pub name: String,
    pub payload: Option<String>,
}

impl UiAction {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            kind: UiActionKind::Click,
            name: name.into(),
            payload: None,
        }
    }

    pub fn kind(mut self, kind: UiActionKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn payload(mut self, payload: impl Into<String>) -> Self {
        self.payload = Some(payload.into());
        self
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct UiDirty: u8 {
        const LAYOUT = 0b0000_0001;
        const PAINT = 0b0000_0010;
        const TEXT = 0b0000_0100;
        const HIERARCHY = 0b0000_1000;
        const FOCUS = 0b0001_0000;
        const ALL = Self::LAYOUT.bits()
            | Self::PAINT.bits()
            | Self::TEXT.bits()
            | Self::HIERARCHY.bits()
            | Self::FOCUS.bits();
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct UiScroll {
    pub offset: UiPoint,
    pub content_size: UiSize,
    pub viewport_size: UiSize,
}

impl UiScroll {
    pub fn max_offset(&self) -> UiPoint {
        UiPoint::new(
            (self.content_size.width - self.viewport_size.width).max(0.0),
            (self.content_size.height - self.viewport_size.height).max(0.0),
        )
    }

    pub fn clamp(&mut self) {
        let max = self.max_offset();
        self.offset.x = self.offset.x.clamp(0.0, max.x);
        self.offset.y = self.offset.y.clamp(0.0, max.y);
    }
}

/// Output of the layout pass.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct UiComputed {
    pub rect: UiRect,
    pub content: UiRect,
    pub clip: Option<UiRect>,
    pub z_index: i32,
    pub visible: bool,
}

impl UiComputed {
    pub fn is_hit(&self, point: UiPoint) -> bool {
        self.visible
            && self.rect.contains(point)
            && self.clip.is_none_or(|clip| clip.contains(point))
    }
}

/// Marker component for a UI root entity.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct UiRoot;

/// Marker component for a scroll container.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct UiScrollContainer;

/// Inline style component, resolved by the style pass into [`UiResolvedStyle`].
pub type UiStyleComponent = UiStyle;
