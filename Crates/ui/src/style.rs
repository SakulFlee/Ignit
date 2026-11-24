//! Style, theme tokens, classes, and resolved paint/text state.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::geometry::UiPoint;

pub type UiRgba = [f32; 4];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiColor {
    Rgba(UiRgba),
    Token(String),
}

impl Default for UiColor {
    fn default() -> Self {
        Self::Rgba([0.0, 0.0, 0.0, 0.0])
    }
}

impl UiColor {
    pub fn rgba(color: UiRgba) -> Self {
        Self::Rgba(color)
    }

    pub fn token(name: impl Into<String>) -> Self {
        Self::Token(name.into())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiCorners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl UiCorners {
    pub const fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiBorder {
    pub width: f32,
    pub color: UiColor,
}

impl Default for UiBorder {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: UiColor::rgba([1.0, 1.0, 1.0, 0.2]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiShadow {
    pub color: UiColor,
    pub offset: UiPoint,
    pub blur: f32,
    pub spread: f32,
}

impl Default for UiShadow {
    fn default() -> Self {
        Self {
            color: UiColor::rgba([0.0, 0.0, 0.0, 0.35]),
            offset: UiPoint::new(0.0, 4.0),
            blur: 12.0,
            spread: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiTextWrap {
    NoWrap,
    #[default]
    Word,
    Glyph,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiTextStyle {
    pub size: f32,
    pub line_height: Option<f32>,
    pub color: UiColor,
    pub family: Option<String>,
    pub weight: u16,
    pub wrap: UiTextWrap,
    pub ellipsis: bool,
}

impl Default for UiTextStyle {
    fn default() -> Self {
        Self {
            size: 14.0,
            line_height: None,
            color: UiColor::token("text"),
            family: None,
            weight: 400,
            wrap: UiTextWrap::Word,
            ellipsis: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiImageStyle {
    pub tint: UiColor,
    pub uv: [f32; 4],
    pub nine_slice: Option<[f32; 4]>,
}

impl Default for UiImageStyle {
    fn default() -> Self {
        Self {
            tint: UiColor::rgba([1.0, 1.0, 1.0, 1.0]),
            uv: [0.0, 0.0, 1.0, 1.0],
            nine_slice: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiCursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    ResizeHorizontal,
    ResizeVertical,
    Grab,
    Grabbing,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiStyle {
    pub background: Option<UiColor>,
    pub border: Option<UiBorder>,
    pub radius: UiCorners,
    pub shadow: Option<UiShadow>,
    pub text: Option<UiTextStyle>,
    pub image: Option<UiImageStyle>,
    pub opacity: Option<f32>,
    pub visible: Option<bool>,
    pub cursor: Option<UiCursorIcon>,
    pub classes: Vec<String>,
}

impl UiStyle {
    pub fn background(mut self, color: UiColor) -> Self {
        self.background = Some(color);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = UiCorners::all(radius);
        self
    }

    pub fn padding_text(mut self, style: UiTextStyle) -> Self {
        self.text = Some(style);
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

/// Partial style used by classes and state overrides.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiStylePatch {
    pub background: Option<UiColor>,
    pub border: Option<UiBorder>,
    pub radius: Option<UiCorners>,
    pub shadow: Option<UiShadow>,
    pub text: Option<UiTextStyle>,
    pub image: Option<UiImageStyle>,
    pub opacity: Option<f32>,
    pub visible: Option<bool>,
    pub cursor: Option<UiCursorIcon>,
}

impl UiStylePatch {
    pub fn apply(&self, style: &mut UiStyle) {
        if let Some(value) = &self.background {
            style.background = Some(value.clone());
        }
        if let Some(value) = &self.border {
            style.border = Some(value.clone());
        }
        if let Some(value) = self.radius {
            style.radius = value;
        }
        if let Some(value) = &self.shadow {
            style.shadow = Some(value.clone());
        }
        if let Some(value) = &self.text {
            style.text = Some(value.clone());
        }
        if let Some(value) = &self.image {
            style.image = Some(value.clone());
        }
        if let Some(value) = self.opacity {
            style.opacity = Some(value);
        }
        if let Some(value) = self.visible {
            style.visible = Some(value);
        }
        if let Some(value) = self.cursor {
            style.cursor = Some(value);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiState {
    #[default]
    Normal,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiStateStyles {
    pub hovered: Option<UiStylePatch>,
    pub pressed: Option<UiStylePatch>,
    pub focused: Option<UiStylePatch>,
    pub disabled: Option<UiStylePatch>,
}

impl UiStateStyles {
    pub fn patch(&self, state: UiState) -> Option<&UiStylePatch> {
        match state {
            UiState::Normal => None,
            UiState::Hovered => self.hovered.as_ref(),
            UiState::Pressed => self.pressed.as_ref(),
            UiState::Focused => self.focused.as_ref(),
            UiState::Disabled => self.disabled.as_ref(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiStyleClass {
    pub style: UiStylePatch,
    pub states: UiStateStyles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiResolvedBorder {
    pub width: f32,
    pub color: UiRgba,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiResolvedShadow {
    pub color: UiRgba,
    pub offset: UiPoint,
    pub blur: f32,
    pub spread: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiResolvedTextStyle {
    pub size: f32,
    pub line_height: Option<f32>,
    pub color: UiRgba,
    pub family: Option<String>,
    pub weight: u16,
    pub wrap: UiTextWrap,
    pub ellipsis: bool,
}

impl From<&UiTextStyle> for UiResolvedTextStyle {
    fn from(value: &UiTextStyle) -> Self {
        Self {
            size: value.size,
            line_height: value.line_height,
            color: [0.0; 4],
            family: value.family.clone(),
            weight: value.weight,
            wrap: value.wrap,
            ellipsis: value.ellipsis,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiResolvedImage {
    pub tint: UiRgba,
    pub uv: [f32; 4],
    pub nine_slice: Option<[f32; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiResolvedStyle {
    pub background: Option<UiRgba>,
    pub border: Option<UiResolvedBorder>,
    pub radius: UiCorners,
    pub shadow: Option<UiResolvedShadow>,
    pub text: Option<UiResolvedTextStyle>,
    pub image: Option<UiResolvedImage>,
    pub opacity: f32,
    pub visible: bool,
    pub cursor: UiCursorIcon,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiText {
    pub value: String,
}

impl UiText {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiImage {
    pub texture: String,
}

impl UiImage {
    pub fn new(texture: impl Into<String>) -> Self {
        Self {
            texture: texture.into(),
        }
    }
}

/// Token-based theme plus style classes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiTheme {
    pub name: String,
    pub revision: u64,
    pub layout_revision: u64,
    pub paint_revision: u64,
    pub colors: HashMap<String, UiRgba>,
    pub typography: HashMap<String, UiTextStyle>,
    pub spacing: HashMap<String, f32>,
    pub radii: HashMap<String, f32>,
    pub borders: HashMap<String, UiBorder>,
    pub shadows: HashMap<String, UiShadow>,
    pub motion: HashMap<String, f32>,
    pub classes: HashMap<String, UiStyleClass>,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl UiTheme {
    pub fn dark() -> Self {
        let mut colors = HashMap::new();
        colors.insert("bg".into(), [0.045, 0.052, 0.075, 1.0]);
        colors.insert("surface".into(), [0.085, 0.10, 0.14, 1.0]);
        colors.insert("surface_alt".into(), [0.11, 0.13, 0.18, 1.0]);
        colors.insert("border".into(), [0.22, 0.25, 0.33, 1.0]);
        colors.insert("text".into(), [0.92, 0.94, 1.0, 1.0]);
        colors.insert("text_muted".into(), [0.60, 0.64, 0.74, 1.0]);
        colors.insert("accent".into(), [0.26, 0.62, 0.92, 1.0]);
        colors.insert("accent_hover".into(), [0.34, 0.70, 0.98, 1.0]);
        colors.insert("info".into(), [0.26, 0.62, 0.92, 1.0]);
        colors.insert("success".into(), [0.30, 0.70, 0.50, 1.0]);
        colors.insert("warning".into(), [0.92, 0.70, 0.22, 1.0]);
        colors.insert("danger".into(), [0.92, 0.32, 0.36, 1.0]);

        let mut spacing = HashMap::new();
        spacing.insert("xs".into(), 4.0);
        spacing.insert("sm".into(), 8.0);
        spacing.insert("md".into(), 12.0);
        spacing.insert("lg".into(), 16.0);
        spacing.insert("xl".into(), 24.0);

        let mut radii = HashMap::new();
        radii.insert("sm".into(), 4.0);
        radii.insert("md".into(), 8.0);
        radii.insert("lg".into(), 12.0);

        let mut typography = HashMap::new();
        typography.insert(
            "body".into(),
            UiTextStyle {
                size: 16.0,
                ..Default::default()
            },
        );
        typography.insert(
            "title".into(),
            UiTextStyle {
                size: 22.0,
                weight: 600,
                ..Default::default()
            },
        );
        typography.insert(
            "caption".into(),
            UiTextStyle {
                size: 13.0,
                color: UiColor::token("text_muted"),
                ..Default::default()
            },
        );

        let mut borders = HashMap::new();
        borders.insert("default".into(), UiBorder::default());

        let mut shadows = HashMap::new();
        shadows.insert("panel".into(), UiShadow::default());

        let mut classes = HashMap::new();
        classes.insert(
            "panel".into(),
            UiStyleClass {
                style: UiStylePatch {
                    background: Some(UiColor::token("surface")),
                    border: Some(UiBorder {
                        width: 1.0,
                        color: UiColor::token("border"),
                    }),
                    radius: Some(UiCorners::all(10.0)),
                    shadow: Some(UiShadow::default()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        classes.insert(
            "muted".into(),
            UiStyleClass {
                style: UiStylePatch {
                    text: Some(UiTextStyle {
                        color: UiColor::token("text_muted"),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        Self {
            name: "dark".into(),
            revision: 1,
            layout_revision: 1,
            paint_revision: 1,
            colors,
            typography,
            spacing,
            radii,
            borders,
            shadows,
            motion: HashMap::new(),
            classes,
        }
    }

    pub fn light() -> Self {
        let mut theme = Self::dark();
        theme.name = "light".into();
        theme.revision += 1;
        theme.paint_revision += 1;
        theme.colors.insert("bg".into(), [0.94, 0.95, 0.98, 1.0]);
        theme.colors.insert("surface".into(), [1.0, 1.0, 1.0, 1.0]);
        theme
            .colors
            .insert("surface_alt".into(), [0.88, 0.90, 0.95, 1.0]);
        theme
            .colors
            .insert("border".into(), [0.55, 0.60, 0.70, 1.0]);
        theme.colors.insert("text".into(), [0.08, 0.10, 0.14, 1.0]);
        theme
            .colors
            .insert("text_muted".into(), [0.30, 0.35, 0.42, 1.0]);
        theme
            .colors
            .insert("accent".into(), [0.10, 0.45, 0.80, 1.0]);
        theme
            .colors
            .insert("accent_hover".into(), [0.16, 0.52, 0.88, 1.0]);
        theme.colors.insert("info".into(), [0.10, 0.45, 0.80, 1.0]);
        theme
            .colors
            .insert("success".into(), [0.12, 0.55, 0.34, 1.0]);
        theme
            .colors
            .insert("warning".into(), [0.80, 0.55, 0.05, 1.0]);
        theme
            .colors
            .insert("danger".into(), [0.78, 0.18, 0.22, 1.0]);
        for (name, style) in &mut theme.typography {
            style.color = if name == "caption" {
                UiColor::rgba([0.30, 0.35, 0.42, 1.0])
            } else {
                UiColor::rgba([0.08, 0.10, 0.14, 1.0])
            };
        }
        if let Some(panel) = theme.classes.get_mut("panel") {
            panel.style.shadow = Some(UiShadow {
                color: UiColor::rgba([0.15, 0.18, 0.25, 0.22]),
                ..Default::default()
            });
        }
        theme
    }

    pub fn from_ron(source: &str) -> Result<Self, String> {
        ron::from_str(source).map_err(|error| error.to_string())
    }

    pub fn to_ron(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())
    }

    pub fn color(&self, name: &str) -> Option<UiRgba> {
        self.colors.get(name).copied()
    }

    pub fn spacing(&self, name: &str) -> Option<f32> {
        self.spacing.get(name).copied()
    }

    pub fn radius(&self, name: &str) -> Option<f32> {
        self.radii.get(name).copied()
    }

    pub fn set_color(&mut self, name: impl Into<String>, color: UiRgba) {
        self.colors.insert(name.into(), color);
        self.revision += 1;
        self.paint_revision += 1;
    }

    pub fn set_spacing(&mut self, name: impl Into<String>, value: f32) {
        self.spacing.insert(name.into(), value);
        self.revision += 1;
        self.layout_revision += 1;
        self.paint_revision += 1;
    }

    pub fn set_typography(&mut self, name: impl Into<String>, style: UiTextStyle) {
        self.typography.insert(name.into(), style);
        self.revision += 1;
        self.layout_revision += 1;
        self.paint_revision += 1;
    }

    pub fn set_class(&mut self, name: impl Into<String>, class: UiStyleClass) {
        self.classes.insert(name.into(), class);
        self.revision += 1;
        self.layout_revision += 1;
        self.paint_revision += 1;
    }

    pub fn resolve_color(&self, color: &UiColor) -> UiRgba {
        match color {
            UiColor::Rgba(color) => *color,
            UiColor::Token(name) => self.color(name).unwrap_or([1.0, 0.0, 1.0, 1.0]),
        }
    }

    pub fn resolve_text(&self, style: &UiTextStyle) -> UiResolvedTextStyle {
        let mut resolved = UiResolvedTextStyle::from(style);
        resolved.color = self.resolve_color(&style.color);
        resolved
    }

    /// Resolve a node's inline style, classes, state, and nearest text token.
    pub fn resolve_style(
        &self,
        style: &UiStyle,
        inherited_classes: &[String],
        state: UiState,
    ) -> UiResolvedStyle {
        let mut merged = UiStyle::default();
        let mut relevant: Vec<&UiStyleClass> = Vec::new();
        for name in inherited_classes.iter().chain(style.classes.iter()) {
            if let Some(class) = self.classes.get(name) {
                relevant.push(class);
                class.style.apply(&mut merged);
            }
        }
        // Inline node style wins over class base values; state overrides win
        // over both so hover/pressed feedback is always visible.
        UiStylePatch {
            background: style.background.clone(),
            border: style.border.clone(),
            radius: Some(style.radius),
            shadow: style.shadow.clone(),
            text: style.text.clone(),
            image: style.image.clone(),
            opacity: style.opacity,
            visible: style.visible,
            cursor: style.cursor,
        }
        .apply(&mut merged);
        for class in &relevant {
            if let Some(patch) = class.states.patch(state) {
                patch.apply(&mut merged);
            }
        }

        let text = match &merged.text {
            Some(style) => Some(self.resolve_text(style)),
            None => self
                .typography
                .get("body")
                .map(|style| self.resolve_text(style)),
        };

        UiResolvedStyle {
            background: merged
                .background
                .as_ref()
                .map(|color| self.resolve_color(color)),
            border: merged.border.as_ref().map(|border| UiResolvedBorder {
                width: border.width,
                color: self.resolve_color(&border.color),
            }),
            radius: merged.radius,
            shadow: merged.shadow.as_ref().map(|shadow| UiResolvedShadow {
                color: self.resolve_color(&shadow.color),
                offset: shadow.offset,
                blur: shadow.blur,
                spread: shadow.spread,
            }),
            text,
            image: merged.image.as_ref().map(|image| UiResolvedImage {
                tint: self.resolve_color(&image.tint),
                uv: image.uv,
                nine_slice: image.nine_slice,
            }),
            opacity: merged.opacity.unwrap_or(1.0),
            visible: merged.visible.unwrap_or(true),
            cursor: merged.cursor.unwrap_or_default(),
        }
    }

    /// Resolve a token name as a text style, falling back to body.
    pub fn text_token(&self, name: &str) -> Option<UiResolvedTextStyle> {
        self.typography
            .get(name)
            .map(|style| self.resolve_text(style))
    }
}

/// Named themes; the active theme is used unless a `UiThemeScope` selects one.
#[derive(Clone, Debug, Default)]
pub struct UiThemes {
    pub active: UiTheme,
    pub named: HashMap<String, UiTheme>,
}

impl UiThemes {
    pub fn with_active(active: UiTheme) -> Self {
        Self {
            active,
            named: HashMap::new(),
        }
    }

    pub fn get(&self, name: Option<&str>) -> &UiTheme {
        name.and_then(|name| self.named.get(name))
            .unwrap_or(&self.active)
    }

    pub fn insert(&mut self, theme: UiTheme) {
        self.named.insert(theme.name.clone(), theme);
    }
}
