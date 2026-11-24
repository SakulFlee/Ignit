//! Engine-level notification overlay.
//!
//! Games post [`UiNotification`] values into the [`UiNotifications`] resource;
//! the UI update pass paints the active cards on top of every root. The
//! overlay is draw-list only: it has no ECS node, never takes part in layout,
//! and never captures pointer input.
//!
//! Notifications are keyed for coalescing. Posting the same key again while
//! the previous entry is still visible refreshes it and increments a repeat
//! counter instead of stacking duplicates, which keeps repeated engine
//! warnings from flooding the screen.

use ignit_ecs::{Entity, World};
use serde::{Deserialize, Serialize};

use crate::draw::{UiDrawCommand, UiDrawList};
use crate::forest::UiForest;
use crate::geometry::{UiPoint, UiRect};
use crate::layout_engine::UiTextMeasure;
use crate::node::UiView;
use crate::style::{UiResolvedTextStyle, UiRgba, UiTextStyle, UiTextWrap, UiTheme, UiThemes};

/// Visual severity of a notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiNotificationSeverity {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl UiNotificationSeverity {
    /// Theme token used for the severity stripe.
    pub fn token(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "danger",
        }
    }

    fn fallback(self) -> UiRgba {
        match self {
            Self::Info => [0.26, 0.62, 0.92, 1.0],
            Self::Success => [0.30, 0.70, 0.50, 1.0],
            Self::Warning => [0.92, 0.70, 0.22, 1.0],
            Self::Error => [0.92, 0.32, 0.36, 1.0],
        }
    }
}

/// The part of the viewport the notification stack is anchored to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiNotificationPlacement {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl UiNotificationPlacement {
    fn is_bottom(self) -> bool {
        matches!(
            self,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight
        )
    }
}

/// One transient notification card.
#[derive(Clone, Debug)]
pub struct UiNotification {
    /// Optional coalescing key. Entries with the same key collapse into one.
    pub key: Option<String>,
    pub severity: UiNotificationSeverity,
    pub title: String,
    pub message: String,
    /// Seconds the card stays visible. Values at or below zero are sticky.
    pub duration: f32,
    /// How many times this keyed notification has been posted.
    pub repeat: u32,
    pub(crate) created_at: f64,
}

impl UiNotification {
    pub fn new(
        severity: UiNotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            key: None,
            severity,
            title: title.into(),
            message: message.into(),
            duration: 5.0,
            repeat: 1,
            created_at: 0.0,
        }
    }

    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(UiNotificationSeverity::Info, title, message)
    }

    pub fn success(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(UiNotificationSeverity::Success, title, message)
    }

    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(UiNotificationSeverity::Warning, title, message)
    }

    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(UiNotificationSeverity::Error, title, message)
    }

    /// Collapses repeated posts of the same key into one card.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn duration(mut self, seconds: f32) -> Self {
        self.duration = seconds;
        self
    }

    /// Keeps the card visible until it is dismissed or evicted.
    pub fn sticky(mut self) -> Self {
        self.duration = 0.0;
        self
    }

    fn is_visible_at(&self, time: f64) -> bool {
        self.duration <= 0.0 || time - self.created_at < self.duration as f64
    }
}

/// Queue of active notifications. Inserted by [`crate::init`].
#[derive(Clone, Debug)]
pub struct UiNotifications {
    entries: Vec<UiNotification>,
    time: f64,
    /// Maximum number of cards painted at once.
    pub max_visible: usize,
    /// Maximum card width in logical pixels.
    pub width: f32,
    /// Distance between the stack and the viewport edge.
    pub margin: f32,
    /// Vertical distance between cards.
    pub gap: f32,
    /// Duration used by [`UiNotifications::notify`].
    pub default_duration: f32,
    pub placement: UiNotificationPlacement,
}

impl Default for UiNotifications {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(8),
            time: 0.0,
            max_visible: 4,
            width: 340.0,
            margin: 16.0,
            gap: 8.0,
            default_duration: 5.0,
            placement: UiNotificationPlacement::default(),
        }
    }
}

impl UiNotifications {
    /// Pre-allocates storage for `capacity` entries. The system-notice path can
    /// use this so showing a fixed engine message does not grow the queue.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    /// Posts a notification, coalescing by key when one is set.
    pub fn post(&mut self, notification: UiNotification) {
        if let Some(key) = notification.key.as_deref()
            && let Some(existing) = self
                .entries
                .iter_mut()
                .find(|entry| entry.key.as_deref() == Some(key))
        {
            existing.severity = notification.severity;
            existing.title = notification.title;
            existing.message = notification.message;
            existing.duration = notification.duration;
            existing.repeat = existing.repeat.saturating_add(1);
            existing.created_at = self.time;
            return;
        }
        let mut notification = notification;
        notification.created_at = self.time;
        self.entries.push(notification);
        self.evict();
    }

    /// Convenience wrapper using [`Self::default_duration`].
    pub fn notify(
        &mut self,
        severity: UiNotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        let duration = self.default_duration;
        self.post(UiNotification::new(severity, title, message).duration(duration));
    }

    /// Advances the clock and drops expired entries.
    pub fn advance(&mut self, delta_seconds: f64) {
        if delta_seconds > 0.0 {
            self.time += delta_seconds;
        }
        self.prune();
    }

    /// Current notification clock in seconds.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Every queued entry, oldest first.
    pub fn entries(&self) -> &[UiNotification] {
        &self.entries
    }

    /// Entries that are currently visible, bounded by [`Self::max_visible`].
    pub fn visible(&self) -> impl Iterator<Item = &UiNotification> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.is_visible_at(self.time))
            .take(self.max_visible)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Removes a keyed entry, including a sticky one.
    pub fn dismiss(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.key.as_deref() != Some(key));
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Reserves room for additional entries.
    pub fn reserve(&mut self, capacity: usize) {
        if capacity > self.entries.capacity() {
            self.entries.reserve(capacity - self.entries.capacity());
        }
    }

    fn prune(&mut self) {
        let time = self.time;
        self.entries.retain(|entry| entry.is_visible_at(time));
    }

    fn evict(&mut self) {
        let limit = self.max_visible.max(1).saturating_mul(2);
        while self.entries.len() > limit {
            self.entries.remove(0);
        }
    }
}

/// Appends the notification overlay to a finished draw list. Commands are
/// inserted after z sorting, so cards paint above every laid-out root.
pub fn append_notification_commands(
    world: &World,
    measure: &mut dyn UiTextMeasure,
    draw: &mut UiDrawList,
) {
    let Some(notifications) = world.get_resource::<UiNotifications>() else {
        return;
    };
    let Some((entity, view)) = overlay_view(world) else {
        return;
    };
    let themes = world.get_resource::<UiThemes>();
    let theme = themes.as_deref().map(|themes| &themes.active);

    let margin = notifications.margin.max(0.0);
    let width = notifications
        .width
        .min((view.size.width - margin * 2.0).max(160.0))
        .max(160.0);
    let gap = notifications.gap.max(0.0);
    let max_height = (view.size.height - margin * 2.0).max(0.0);

    let title_base = UiTextStyle {
        size: 15.0,
        weight: 600,
        wrap: UiTextWrap::Word,
        ..Default::default()
    };
    let message_base = UiTextStyle {
        size: 13.0,
        wrap: UiTextWrap::Word,
        ..Default::default()
    };
    let title_style = resolve_text(theme, &title_base);
    let message_style = resolve_text(theme, &message_base);
    let background = token_color(theme, "surface", [0.09, 0.10, 0.14, 1.0]);
    let border = token_color(theme, "border", [0.22, 0.25, 0.33, 1.0]);
    let shadow = theme
        .and_then(|theme| {
            theme
                .shadows
                .get("panel")
                .map(|shadow| theme.resolve_color(&shadow.color))
        })
        .unwrap_or([0.0, 0.0, 0.0, 0.28]);

    let pad = 12.0;
    let stripe = 4.0;
    let text_x_offset = stripe + 12.0;
    let text_width = (width - text_x_offset - pad).max(1.0);

    let x = match notifications.placement {
        UiNotificationPlacement::TopLeft | UiNotificationPlacement::BottomLeft => {
            view.offset.x + margin
        }
        UiNotificationPlacement::TopCenter | UiNotificationPlacement::BottomCenter => {
            view.offset.x + ((view.size.width - width) * 0.5).max(0.0)
        }
        UiNotificationPlacement::TopRight | UiNotificationPlacement::BottomRight => {
            view.offset.x + (view.size.width - width - margin).max(0.0)
        }
    };

    let mut cards = Vec::new();
    let mut total = 0.0;
    for notification in notifications.visible() {
        let mut title = notification.title.clone();
        if notification.repeat > 1 {
            title.push_str(&format!("  \u{00d7}{}", notification.repeat));
        }
        let title_height = if title.is_empty() {
            0.0
        } else {
            measure
                .measure(&title, &title_base, Some(text_width))
                .height
        };
        let message_height = if notification.message.is_empty() {
            0.0
        } else {
            measure
                .measure(&notification.message, &message_base, Some(text_width))
                .height
        };
        let mut text_height = title_height;
        if message_height > 0.0 {
            if text_height > 0.0 {
                text_height += 4.0;
            }
            text_height += message_height;
        }
        let height = (pad * 2.0 + text_height).max(pad * 2.0);
        if total + height > max_height && !cards.is_empty() {
            break;
        }
        cards.push((notification, title, title_height, message_height, height));
        total += height + gap;
    }
    if total > 0.0 {
        total -= gap;
    }

    let mut y = if notifications.placement.is_bottom() {
        view.offset.y + (view.size.height - margin - total).max(0.0)
    } else {
        view.offset.y + margin
    };

    for (notification, title, title_height, message_height, height) in cards {
        let rect = UiRect::new(x, y, width, height);
        let color = token_color(
            theme,
            notification.severity.token(),
            notification.severity.fallback(),
        );
        draw.commands.push(UiDrawCommand::Shadow {
            entity,
            rect,
            color: shadow,
            offset: UiPoint::new(0.0, 4.0),
            blur: 12.0,
            spread: 0.0,
            radius: [8.0; 4],
            clip: None,
        });
        draw.commands.push(UiDrawCommand::Rect {
            entity,
            rect,
            color: background,
            radius: [8.0; 4],
            clip: None,
        });
        draw.commands.push(UiDrawCommand::Border {
            entity,
            rect,
            width: 1.0,
            color: border,
            radius: [8.0; 4],
            clip: None,
        });
        draw.commands.push(UiDrawCommand::Rect {
            entity,
            rect: UiRect::new(rect.x, rect.y, stripe, rect.height),
            color,
            radius: [8.0, 0.0, 0.0, 8.0],
            clip: None,
        });

        let text_x = rect.x + text_x_offset;
        let mut text_y = rect.y + pad;
        if title_height > 0.0 {
            draw.commands.push(UiDrawCommand::Text {
                entity,
                rect: UiRect::new(text_x, text_y, text_width, title_height),
                text: title,
                style: title_style.clone(),
                clip: None,
                caret: None,
            });
            text_y += title_height;
        }
        if message_height > 0.0 {
            if title_height > 0.0 {
                text_y += 4.0;
            }
            draw.commands.push(UiDrawCommand::Text {
                entity,
                rect: UiRect::new(text_x, text_y, text_width, message_height),
                text: notification.message.clone(),
                style: message_style.clone(),
                clip: None,
                caret: None,
            });
        }

        y += height + gap;
    }
}

fn resolve_text(theme: Option<&UiTheme>, style: &UiTextStyle) -> UiResolvedTextStyle {
    match theme {
        Some(theme) => theme.resolve_text(style),
        None => {
            let mut style = style.clone();
            style.color = crate::style::UiColor::rgba([0.92, 0.94, 1.0, 1.0]);
            UiResolvedTextStyle::from(&style)
        }
    }
}

fn token_color(theme: Option<&UiTheme>, name: &str, fallback: UiRgba) -> UiRgba {
    theme
        .and_then(|themes| themes.color(name))
        .unwrap_or(fallback)
}

/// Largest visible root, used as the screen-space anchor. Viewport order is
/// only a tie-breaker so small overlay roots cannot capture the toast stack.
fn overlay_view(world: &World) -> Option<(Entity, UiView)> {
    let forest = world.get_resource::<UiForest>()?;
    let views = world.get_component_store::<UiView>()?;
    forest
        .roots()
        .iter()
        .filter_map(|root| {
            views
                .get_component(root.index)
                .filter(|view| view.visible)
                .map(|view| (*root, *view))
        })
        .max_by(|(_, left), (_, right)| {
            (left.size.width * left.size.height)
                .total_cmp(&(right.size.width * right.size.height))
                .then_with(|| left.order.cmp(&right.order))
        })
}
