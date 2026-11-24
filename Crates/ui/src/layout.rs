//! Serializable layout style and the Taffy bridge.

use serde::{Deserialize, Serialize};
use taffy::prelude::*;
use taffy::style_helpers::TaffyGridLine;
use taffy::{MaxTrackSizingFunction, MinTrackSizingFunction, TrackSizingFunction};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiDisplay {
    #[default]
    Flex,
    Grid,
    Block,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFlexDirection {
    Row,
    #[default]
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiJustify {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiAlign {
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UiDimension {
    #[default]
    Auto,
    Length(f32),
    Percent(f32),
}

impl UiDimension {
    pub const ZERO: Self = Self::Length(0.0);
    pub const FULL: Self = Self::Percent(100.0);

    pub fn resolve(self, parent: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Length(value) => Some(value),
            Self::Percent(value) => Some(parent * value / 100.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UiLengthPercentage {
    #[default]
    Zero,
    Length(f32),
    Percent(f32),
}

impl UiLengthPercentage {
    pub const fn length(value: f32) -> Self {
        Self::Length(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiEdges<T> {
    pub left: T,
    pub right: T,
    pub top: T,
    pub bottom: T,
}

impl<T: Copy> UiEdges<T> {
    pub const fn all(value: T) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: T, vertical: T) -> Self {
        Self {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }
}

impl UiEdges<UiLengthPercentage> {
    pub fn length(value: f32) -> Self {
        Self::all(UiLengthPercentage::Length(value))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiGap {
    pub width: UiLengthPercentage,
    pub height: UiLengthPercentage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPosition {
    #[default]
    Relative,
    Absolute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiOverflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiGridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UiTrack {
    #[default]
    Auto,
    Length(f32),
    Percent(f32),
    Fr(f32),
    MinMax(Box<UiTrack>, Box<UiTrack>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiGridPlacement {
    #[default]
    Auto,
    Line(i16),
    Span(u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiGridLine {
    pub start: UiGridPlacement,
    pub end: UiGridPlacement,
}

/// Serializable layout description, bridged to [`taffy::Style`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiLayout {
    pub display: UiDisplay,
    pub direction: UiFlexDirection,
    pub wrap: UiFlexWrap,
    pub justify_content: UiJustify,
    pub align_items: UiAlign,
    pub align_self: Option<UiAlign>,
    pub width: UiDimension,
    pub height: UiDimension,
    pub min_width: UiDimension,
    pub min_height: UiDimension,
    pub max_width: UiDimension,
    pub max_height: UiDimension,
    pub padding: UiEdges<UiLengthPercentage>,
    pub margin: UiEdges<UiDimension>,
    pub gap: UiGap,
    pub position: UiPosition,
    pub inset: UiEdges<UiDimension>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: UiDimension,
    pub overflow: UiOverflow,
    pub grid_template_columns: Vec<UiTrack>,
    pub grid_template_rows: Vec<UiTrack>,
    pub grid_auto_flow: UiGridAutoFlow,
    pub grid_column: UiGridLine,
    pub grid_row: UiGridLine,
}

impl Default for UiLayout {
    fn default() -> Self {
        Self {
            display: UiDisplay::Flex,
            direction: UiFlexDirection::Column,
            wrap: UiFlexWrap::NoWrap,
            justify_content: UiJustify::Start,
            align_items: UiAlign::Stretch,
            align_self: None,
            width: UiDimension::Auto,
            height: UiDimension::Auto,
            min_width: UiDimension::Auto,
            min_height: UiDimension::Auto,
            max_width: UiDimension::Auto,
            max_height: UiDimension::Auto,
            padding: UiEdges::all(UiLengthPercentage::Zero),
            margin: UiEdges::all(UiDimension::Length(0.0)),
            gap: UiGap {
                width: UiLengthPercentage::Zero,
                height: UiLengthPercentage::Zero,
            },
            position: UiPosition::Relative,
            inset: UiEdges::all(UiDimension::Auto),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: UiDimension::Auto,
            overflow: UiOverflow::Visible,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_flow: UiGridAutoFlow::Row,
            grid_column: UiGridLine::default(),
            grid_row: UiGridLine::default(),
        }
    }
}

impl UiLayout {
    pub fn row() -> Self {
        Self {
            direction: UiFlexDirection::Row,
            ..Default::default()
        }
    }

    pub fn column() -> Self {
        Self::default()
    }

    pub fn display(mut self, display: UiDisplay) -> Self {
        self.display = display;
        self
    }

    pub fn direction(mut self, direction: UiFlexDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn justify(mut self, justify: UiJustify) -> Self {
        self.justify_content = justify;
        self
    }

    pub fn align_items(mut self, align: UiAlign) -> Self {
        self.align_items = align;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = UiDimension::Length(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = UiDimension::Length(height);
        self
    }

    pub fn width_percent(mut self, percent: f32) -> Self {
        self.width = UiDimension::Percent(percent);
        self
    }

    pub fn height_percent(mut self, percent: f32) -> Self {
        self.height = UiDimension::Percent(percent);
        self
    }

    pub fn width_full(self) -> Self {
        self.width_percent(100.0)
    }

    pub fn height_full(self) -> Self {
        self.height_percent(100.0)
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = UiDimension::Length(width);
        self.height = UiDimension::Length(height);
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = UiEdges::length(padding);
        self
    }

    pub fn padding_edges(mut self, padding: UiEdges<UiLengthPercentage>) -> Self {
        self.padding = padding;
        self
    }

    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = UiEdges::all(UiDimension::Length(margin));
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = UiGap {
            width: UiLengthPercentage::Length(gap),
            height: UiLengthPercentage::Length(gap),
        };
        self
    }

    pub fn grow(mut self) -> Self {
        self.flex_grow = 1.0;
        self
    }

    pub fn shrink(mut self, shrink: f32) -> Self {
        self.flex_shrink = shrink;
        self
    }

    pub fn flex_basis(mut self, basis: UiDimension) -> Self {
        self.flex_basis = basis;
        self
    }

    pub fn absolute(mut self) -> Self {
        self.position = UiPosition::Absolute;
        self
    }

    pub fn inset(mut self, inset: UiEdges<UiDimension>) -> Self {
        self.inset = inset;
        self
    }

    pub fn overflow(mut self, overflow: UiOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn to_taffy(&self) -> Style {
        Style {
            display: match self.display {
                UiDisplay::Flex => Display::Flex,
                UiDisplay::Grid => Display::Grid,
                UiDisplay::Block => Display::Block,
                UiDisplay::None => Display::None,
            },
            flex_direction: match self.direction {
                UiFlexDirection::Row => FlexDirection::Row,
                UiFlexDirection::Column => FlexDirection::Column,
                UiFlexDirection::RowReverse => FlexDirection::RowReverse,
                UiFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
            },
            flex_wrap: match self.wrap {
                UiFlexWrap::NoWrap => FlexWrap::NoWrap,
                UiFlexWrap::Wrap => FlexWrap::Wrap,
                UiFlexWrap::WrapReverse => FlexWrap::WrapReverse,
            },
            justify_content: match self.justify_content {
                UiJustify::Start => JustifyContent::START,
                UiJustify::End => JustifyContent::END,
                UiJustify::Center => JustifyContent::CENTER,
                UiJustify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                UiJustify::SpaceAround => JustifyContent::SPACE_AROUND,
                UiJustify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
            },
            align_items: align_items(self.align_items),
            align_self: self.align_self.map(align_self),
            size: Size {
                width: dimension(self.width),
                height: dimension(self.height),
            },
            min_size: Size {
                width: dimension_auto(self.min_width),
                height: dimension_auto(self.min_height),
            },
            max_size: Size {
                width: dimension_auto(self.max_width),
                height: dimension_auto(self.max_height),
            },
            padding: edges(self.padding, length_percentage),
            margin: edges(self.margin, dimension_auto),
            gap: Size {
                width: length_percentage(self.gap.width),
                height: length_percentage(self.gap.height),
            },
            position: match self.position {
                UiPosition::Relative => Position::Relative,
                UiPosition::Absolute => Position::Absolute,
            },
            inset: edges(self.inset, dimension_auto),
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: dimension(self.flex_basis),
            overflow: taffy::Point {
                x: overflow(self.overflow),
                y: overflow(self.overflow),
            },
            grid_template_columns: self
                .grid_template_columns
                .iter()
                .map(|track| taffy::GridTemplateComponent::Single(track_sizing(track)))
                .collect(),
            grid_template_rows: self
                .grid_template_rows
                .iter()
                .map(|track| taffy::GridTemplateComponent::Single(track_sizing(track)))
                .collect(),
            grid_auto_flow: match self.grid_auto_flow {
                UiGridAutoFlow::Row => GridAutoFlow::Row,
                UiGridAutoFlow::Column => GridAutoFlow::Column,
                UiGridAutoFlow::RowDense => GridAutoFlow::RowDense,
                UiGridAutoFlow::ColumnDense => GridAutoFlow::ColumnDense,
            },
            grid_column: grid_line(self.grid_column),
            grid_row: grid_line(self.grid_row),
            ..Default::default()
        }
    }
}

fn dimension(value: UiDimension) -> Dimension {
    match value {
        UiDimension::Auto => Dimension::auto(),
        UiDimension::Length(value) => Dimension::length(value),
        UiDimension::Percent(value) => Dimension::percent(value),
    }
}

fn dimension_auto(value: UiDimension) -> LengthPercentageAuto {
    match value {
        UiDimension::Auto => auto(),
        UiDimension::Length(value) => length(value),
        UiDimension::Percent(value) => percent(value),
    }
}

fn length_percentage(value: UiLengthPercentage) -> LengthPercentage {
    match value {
        UiLengthPercentage::Zero => length(0.0),
        UiLengthPercentage::Length(value) => length(value),
        UiLengthPercentage::Percent(value) => percent(value),
    }
}

fn edges<T: Copy, U>(value: UiEdges<T>, map: impl Fn(T) -> U) -> taffy::Rect<U> {
    taffy::Rect {
        left: map(value.left),
        right: map(value.right),
        top: map(value.top),
        bottom: map(value.bottom),
    }
}

fn align_items(value: UiAlign) -> AlignItems {
    match value {
        UiAlign::Start => AlignItems::START,
        UiAlign::End => AlignItems::END,
        UiAlign::FlexStart => AlignItems::FLEX_START,
        UiAlign::FlexEnd => AlignItems::FLEX_END,
        UiAlign::Center => AlignItems::CENTER,
        UiAlign::Baseline => AlignItems::BASELINE,
        UiAlign::Stretch => AlignItems::STRETCH,
    }
}

fn align_self(value: UiAlign) -> AlignSelf {
    match value {
        UiAlign::Start => AlignSelf::START,
        UiAlign::End => AlignSelf::END,
        UiAlign::FlexStart => AlignSelf::FLEX_START,
        UiAlign::FlexEnd => AlignSelf::FLEX_END,
        UiAlign::Center => AlignSelf::CENTER,
        UiAlign::Baseline => AlignSelf::BASELINE,
        UiAlign::Stretch => AlignSelf::STRETCH,
    }
}

fn overflow(value: UiOverflow) -> taffy::Overflow {
    match value {
        UiOverflow::Visible => taffy::Overflow::Visible,
        UiOverflow::Hidden => taffy::Overflow::Hidden,
        UiOverflow::Scroll => taffy::Overflow::Scroll,
    }
}

fn track_sizing(value: &UiTrack) -> TrackSizingFunction {
    match value {
        UiTrack::Auto => TrackSizingFunction {
            min: MinTrackSizingFunction::auto(),
            max: MaxTrackSizingFunction::auto(),
        },
        UiTrack::Length(value) => TrackSizingFunction {
            min: MinTrackSizingFunction::length(*value),
            max: MaxTrackSizingFunction::length(*value),
        },
        UiTrack::Percent(value) => TrackSizingFunction {
            min: MinTrackSizingFunction::percent(*value),
            max: MaxTrackSizingFunction::percent(*value),
        },
        UiTrack::Fr(value) => TrackSizingFunction {
            min: MinTrackSizingFunction::auto(),
            max: MaxTrackSizingFunction::fr(*value),
        },
        UiTrack::MinMax(min, max) => TrackSizingFunction {
            min: min_track(min),
            max: max_track(max),
        },
    }
}

fn min_track(value: &UiTrack) -> MinTrackSizingFunction {
    match value {
        UiTrack::Auto | UiTrack::Fr(_) => MinTrackSizingFunction::auto(),
        UiTrack::Length(value) => MinTrackSizingFunction::length(*value),
        UiTrack::Percent(value) => MinTrackSizingFunction::percent(*value),
        UiTrack::MinMax(min, _) => min_track(min),
    }
}

fn max_track(value: &UiTrack) -> MaxTrackSizingFunction {
    match value {
        UiTrack::Auto => MaxTrackSizingFunction::auto(),
        UiTrack::Length(value) => MaxTrackSizingFunction::length(*value),
        UiTrack::Percent(value) => MaxTrackSizingFunction::percent(*value),
        UiTrack::Fr(value) => MaxTrackSizingFunction::fr(*value),
        UiTrack::MinMax(_, max) => max_track(max),
    }
}

fn grid_line(value: UiGridLine) -> Line<taffy::GridPlacement> {
    Line {
        start: grid_placement(value.start),
        end: grid_placement(value.end),
    }
}

fn grid_placement(value: UiGridPlacement) -> taffy::GridPlacement {
    match value {
        UiGridPlacement::Auto => taffy::GridPlacement::Auto,
        UiGridPlacement::Line(line) => taffy::GridPlacement::from_line_index(line),
        UiGridPlacement::Span(span) => taffy::GridPlacement::Span(span),
    }
}
