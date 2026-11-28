//! Taffy layout bridge and computed-layout output.

use std::collections::HashMap;
use std::sync::Mutex;

use ignit_ecs::{Entity, World};
use taffy::AvailableSpace as TaffyAvailable;
use taffy::prelude::*;

use crate::floating::clamp_floating_window;
use crate::forest::UiForest;
use crate::geometry::{UiRect, UiSize};
use crate::layout::{UiDimension, UiLayout};
use crate::node::{UiComputed, UiDirty, UiNode, UiScroll, UiView};
use crate::style::{UiResolvedStyle, UiStyle, UiText, UiTextStyle, UiThemes};

/// Measures shaped text for the layout pass.
///
/// `ignit_ui_text` implements this. The core ships a deterministic fallback so
/// headless construction and tests do not need a font stack.
pub trait UiTextMeasure {
    fn measure(&mut self, text: &str, style: &UiTextStyle, max_width: Option<f32>) -> UiSize;

    /// Maps a point inside a text run to a byte cursor index.
    fn cursor_index(
        &mut self,
        text: &str,
        style: &UiTextStyle,
        _max_width: f32,
        x: f32,
        _y: f32,
    ) -> usize {
        let char_width = (style.size * 0.55).max(1.0);
        let index = (x / char_width).round().max(0.0) as usize;
        text.char_indices()
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or(text.len())
    }

    /// Byte range of the visual line containing `cursor`.
    fn line_bounds(
        &mut self,
        text: &str,
        _style: &UiTextStyle,
        _max_width: f32,
        cursor: usize,
    ) -> (usize, usize) {
        let cursor = cursor.min(text.len());
        let start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
        let end = text[cursor..]
            .find('\n')
            .map_or(text.len(), |index| cursor + index);
        (start, end)
    }

    /// Cursor index one visual line above/below `cursor`.
    fn vertical_cursor(
        &mut self,
        text: &str,
        style: &UiTextStyle,
        max_width: f32,
        cursor: usize,
        up: bool,
        _goal_x: f32,
    ) -> usize {
        let (start, end) = self.line_bounds(text, style, max_width, cursor);
        if up {
            if start == 0 {
                return cursor;
            }
            let previous_end = start - 1;
            let previous_start = text[..previous_end]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let column = text[start..cursor].chars().count();
            text[previous_start..previous_end]
                .char_indices()
                .nth(column)
                .map_or(previous_end, |(offset, _)| previous_start + offset)
        } else {
            if end == text.len() {
                return cursor;
            }
            let next_start = end + 1;
            let next_end = text[next_start..]
                .find('\n')
                .map_or(text.len(), |index| next_start + index);
            let column = text[start..cursor].chars().count();
            text[next_start..next_end]
                .char_indices()
                .nth(column)
                .map_or(next_end, |(offset, _)| next_start + offset)
        }
    }

    /// Caret rectangle relative to the text content.
    fn cursor_rect(
        &mut self,
        text: &str,
        style: &UiTextStyle,
        _max_width: f32,
        cursor: usize,
    ) -> Option<[f32; 4]> {
        let cursor = cursor.min(text.len());
        let before = &text[..cursor];
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let line = before.matches('\n').count();
        let char_width = (style.size * 0.55).max(1.0);
        let x = text[line_start..cursor].chars().count() as f32 * char_width;
        let height = style.line_height.unwrap_or(style.size * 1.25);
        Some([x, line as f32 * height, 1.0, height])
    }
}

#[derive(Default)]
pub struct UiFallbackTextMeasure;

impl UiTextMeasure for UiFallbackTextMeasure {
    fn measure(&mut self, text: &str, style: &UiTextStyle, max_width: Option<f32>) -> UiSize {
        let char_width = style.size * 0.55;
        let natural = text.chars().count() as f32 * char_width;
        let width = max_width.map_or(natural, |max| natural.min(max));
        let line_height = style.line_height.unwrap_or(style.size * 1.25);
        let lines = if max_width.is_some() && natural > 0.0 {
            (natural / max_width.unwrap_or(1.0).max(1.0)).ceil()
        } else {
            1.0
        };
        UiSize::new(width.ceil(), (lines * line_height).ceil())
    }
}

#[derive(Clone, Debug)]
struct TextMeasureData {
    text: String,
    style: UiTextStyle,
}

#[derive(Clone, Debug)]
struct LayoutNode {
    entity: Entity,
    text: Option<TextMeasureData>,
}

/// Inherited clipping for a layout node.
///
/// `clip` is the effective content boundary. `overflow` is the same boundary
/// with floating-window clipping removed so open dropdowns can keep painting
/// their option list past the window edge.
#[derive(Clone, Copy, Debug, Default)]
struct ClipState {
    clip: Option<UiRect>,
    overflow: Option<UiRect>,
}

/// Intersects `bounds` with an inherited clip without dropping the clip when
/// the two rectangles do not overlap. Falling back to `bounds` would let an
/// overflowing scroll container or window paint outside its ancestor again.
fn clip_to_parent(bounds: UiRect, parent: Option<UiRect>) -> UiRect {
    match parent {
        Some(parent) => bounds
            .intersect(parent)
            .unwrap_or(UiRect::new(bounds.x, bounds.y, 0.0, 0.0)),
        None => bounds,
    }
}

pub struct UiLayoutEngine {
    taffy: TaffyTree<LayoutNode>,
    node_ids: HashMap<Entity, NodeId>,
    roots: Vec<(Entity, NodeId, i32)>,
    last_forest_revision: u64,
    last_layout_revision: u64,
    last_viewports: Vec<(Entity, UiSize)>,
    frames: u64,
    periodic_interval: u64,
}

// Taffy 0.15 stores numeric lengths in a tagged `*const ()` inside
// `CompactLength`, so `TaffyTree` is not `Send`/`Sync`. The engine never leaves
// the UI thread; it is stored inside a `Mutex` so the ECS resource bound is
// satisfied without sharing the tree across threads. Do not remove this
// invariant without re-checking Taffy's pointer representation.
unsafe impl Send for UiLayoutEngine {}

impl Default for UiLayoutEngine {
    fn default() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_ids: HashMap::new(),
            roots: Vec::new(),
            last_forest_revision: 0,
            last_layout_revision: 0,
            last_viewports: Vec::new(),
            frames: 0,
            periodic_interval: 30,
        }
    }
}

impl UiLayoutEngine {
    pub fn periodic_interval(&self) -> u64 {
        self.periodic_interval
    }

    pub fn set_periodic_interval(&mut self, frames: u64) {
        self.periodic_interval = frames.max(1);
    }

    /// Whether the next layout pass has work to do.
    pub fn needs_layout(&self, world: &World) -> bool {
        self.frames.is_multiple_of(self.periodic_interval)
            || world
                .get_resource::<UiForest>()
                .is_some_and(|forest| forest.revision() != self.last_forest_revision)
            || theme_layout_revision(world) != self.last_layout_revision
            || self.viewports_changed(world)
            || any_layout_dirty(world)
    }

    fn viewports_changed(&self, world: &World) -> bool {
        let Some(forest) = world.get_resource::<UiForest>() else {
            return false;
        };
        let views = world.get_component_store::<UiView>();
        let current: Vec<(Entity, UiSize)> = forest
            .roots()
            .iter()
            .filter_map(|entity| {
                views
                    .as_ref()
                    .and_then(|store| store.get_component(entity.index))
                    .map(|view| (*entity, view.size))
            })
            .collect();
        current != self.last_viewports
    }

    /// Rebuilds the Taffy tree and writes [`UiComputed`] components.
    ///
    /// Returns `true` when a layout pass ran.
    pub fn compute(&mut self, world: &mut World, measure: &mut dyn UiTextMeasure) -> bool {
        self.frames = self.frames.wrapping_add(1);
        if !self.needs_layout(world) {
            return false;
        }

        let forest_roots = world
            .get_resource::<UiForest>()
            .map(|forest| forest.roots().to_vec())
            .unwrap_or_default();
        let forest_revision = world
            .get_resource::<UiForest>()
            .map(|forest| forest.revision())
            .unwrap_or(0);

        let mut passes = 0;
        loop {
            passes += 1;
            self.taffy = TaffyTree::new();
            self.node_ids.clear();
            self.roots.clear();

            {
                let world_ref: &World = &*world;
                let themes = world_ref.get_resource::<UiThemes>();
                for &root_entity in &forest_roots {
                    let view = world_ref
                        .get_component_store::<UiView>()
                        .and_then(|store| store.get_component(root_entity.index).copied());
                    let Some(view) = view else {
                        continue;
                    };
                    if !view.visible {
                        continue;
                    }
                    let Some(root_node) =
                        self.build_node(world_ref, root_entity, themes.as_deref())
                    else {
                        continue;
                    };
                    self.roots.push((root_entity, root_node, view.order));
                    self.taffy
                        .compute_layout_with_measure(
                            root_node,
                            Size {
                                width: TaffyAvailable::Definite(view.size.width.max(1.0)),
                                height: TaffyAvailable::Definite(view.size.height.max(1.0)),
                            },
                            |inputs, _node_id, context, style| {
                                let Some(context) = context else {
                                    return taffy::compute_leaf_layout(
                                        inputs,
                                        style,
                                        |_, _| 0.0,
                                        |_, _| Size::ZERO,
                                    );
                                };
                                let Some(text) = &context.text else {
                                    return taffy::compute_leaf_layout(
                                        inputs,
                                        style,
                                        |_, _| 0.0,
                                        |_, _| Size::ZERO,
                                    );
                                };
                                taffy::compute_leaf_layout(
                                    inputs,
                                    style,
                                    |_, _| 0.0,
                                    |known, available| {
                                        let max_width = match available.width {
                                            TaffyAvailable::Definite(width) => Some(width),
                                            _ => known.width,
                                        };
                                        let measured =
                                            measure.measure(&text.text, &text.style, max_width);
                                        Size {
                                            width: known.width.unwrap_or(measured.width),
                                            height: known.height.unwrap_or(measured.height),
                                        }
                                    },
                                )
                            },
                        )
                        .unwrap();
                }
            }

            let roots = self.roots.clone();
            for (root_entity, root_node, order) in roots {
                let z_base = order.saturating_mul(10_000);
                let origin = world
                    .get_component_store::<UiView>()
                    .and_then(|store| {
                        store
                            .get_component(root_entity.index)
                            .map(|view| (view.offset.x, view.offset.y))
                    })
                    .unwrap_or((0.0, 0.0));
                self.write_computed(world, root_node, origin, ClipState::default(), z_base, true);
            }

            let clamped = clamp_floating_windows(world);
            if !clamped || passes >= 4 {
                break;
            }
        }

        let mut last_views = Vec::new();
        {
            let views = world.get_component_store::<UiView>();
            for (root_entity, _, _) in &self.roots {
                if let Some(view) = views
                    .as_ref()
                    .and_then(|store| store.get_component(root_entity.index))
                {
                    last_views.push((*root_entity, view.size));
                }
            }
        }
        self.last_forest_revision = forest_revision;
        self.last_layout_revision = theme_layout_revision(world);
        self.last_viewports = last_views;

        clear_layout_dirty(world);
        true
    }

    fn build_node(
        &mut self,
        world: &World,
        entity: Entity,
        themes: Option<&UiThemes>,
    ) -> Option<NodeId> {
        let node = world
            .get_component_store::<UiNode>()?
            .get_component(entity.index)?
            .clone();
        let layout = world
            .get_component_store::<UiLayout>()
            .and_then(|store| store.get_component(entity.index).cloned())
            .unwrap_or_default();
        let visible = world
            .get_component_store::<UiStyle>()
            .and_then(|store| {
                store
                    .get_component(entity.index)
                    .and_then(|style| style.visible)
            })
            .unwrap_or(true);
        let mut taffy_style = layout.to_taffy();
        if !visible {
            taffy_style.display = taffy::Display::None;
        }
        let is_modal = world
            .get_component_store::<crate::widgets::UiWidget>()
            .and_then(|store| {
                store.get_component(entity.index).map(|widget| {
                    matches!(&widget.spec, crate::widgets::UiWidgetSpec::Modal { .. })
                })
            })
            .unwrap_or(false);
        if is_modal {
            let mut root = entity;
            let nodes = world.get_component_store::<UiNode>();
            while let Some(parent) = nodes
                .as_ref()
                .and_then(|store| store.get_component(root.index))
                .and_then(|node| node.parent)
            {
                root = parent;
            }
            if let Some(view_size) = world
                .get_component_store::<UiView>()
                .and_then(|store| store.get_component(root.index).map(|view| view.size))
            {
                let width = resolved_dimension(layout.width, view_size.width);
                let height = resolved_dimension(layout.height, view_size.height);
                let x = ((view_size.width - width) * 0.5).max(0.0);
                let y = ((view_size.height - height) * 0.5).max(0.0);
                taffy_style.position = Position::Absolute;
                taffy_style.inset = taffy::Rect {
                    left: length(x),
                    right: auto(),
                    top: length(y),
                    bottom: auto(),
                };
            }
        }

        let text = world
            .get_component_store::<UiText>()
            .and_then(|store| store.get_component(entity.index).cloned())
            .map(|text| {
                let style = text_style(world, entity, themes);
                TextMeasureData {
                    text: text.value,
                    style,
                }
            });

        let taffy_node = self
            .taffy
            .new_leaf_with_context(taffy_style, LayoutNode { entity, text })
            .ok()?;
        self.node_ids.insert(entity, taffy_node);

        for child in &node.children {
            if let Some(child_node) = self.build_node(world, *child, themes) {
                let _ = self.taffy.add_child(taffy_node, child_node);
            }
        }
        Some(taffy_node)
    }

    fn write_computed(
        &self,
        world: &mut World,
        node: NodeId,
        origin: (f32, f32),
        clips: ClipState,
        z_index: i32,
        parent_visible: bool,
    ) {
        let Ok(layout) = self.taffy.layout(node) else {
            return;
        };
        let Some(context) = self.taffy.get_node_context(node) else {
            return;
        };
        let entity = context.entity;
        let parent_clip = clips.clip;
        let overflow_clip = clips.overflow;
        let widget = world
            .get_component_store::<crate::widgets::UiWidget>()
            .and_then(|store| store.get_component(entity.index).cloned());
        let is_floating_window = widget.as_ref().is_some_and(|widget| {
            matches!(
                widget.spec,
                crate::widgets::UiWidgetSpec::FloatingWindow { .. }
            )
        });
        let is_open_dropdown = widget.as_ref().is_some_and(|widget| {
            matches!(
                widget.spec,
                crate::widgets::UiWidgetSpec::Dropdown { open: true, .. }
            )
        });
        let z_index = widget
            .as_ref()
            .map(|widget| match &widget.spec {
                crate::widgets::UiWidgetSpec::FloatingWindow { .. } => {
                    // Keep every child of one window inside a large z band
                    // so another window's rows cannot interleave with it.
                    z_index.saturating_mul(1000) + 100_000 + widget.raise
                }
                crate::widgets::UiWidgetSpec::Modal { .. } => z_index + 1_000_000,
                crate::widgets::UiWidgetSpec::Dropdown { open: true, .. } => z_index + 200,
                // Tab labels sit directly above their content panels. Keep
                // them above panel shadows/borders that could paint over the
                // text after a scroll-layout change.
                crate::widgets::UiWidgetSpec::Tabs { .. } => z_index + 50,
                _ => z_index,
            })
            .unwrap_or(z_index);
        let abs = UiRect::new(
            origin.0 + layout.location.x,
            origin.1 + layout.location.y,
            layout.size.width,
            layout.size.height,
        );
        let content = UiRect::new(
            abs.x + layout.border.left + layout.padding.left,
            abs.y + layout.border.top + layout.padding.top,
            (abs.width
                - layout.border.left
                - layout.border.right
                - layout.padding.left
                - layout.padding.right)
                .max(0.0),
            (abs.height
                - layout.border.top
                - layout.border.bottom
                - layout.padding.top
                - layout.padding.bottom)
                .max(0.0),
        );

        let children = self.taffy.children(node).unwrap_or_default();
        let mut content_width = abs.width;
        let mut content_height = abs.height;
        for child in &children {
            if let Ok(child_layout) = self.taffy.layout(*child) {
                content_width = content_width
                    .max(child_layout.location.x + child_layout.size.width + layout.padding.right);
                content_height = content_height.max(
                    child_layout.location.y + child_layout.size.height + layout.padding.bottom,
                );
            }
        }

        let mut scroll = world
            .get_component_store::<UiScroll>()
            .and_then(|store| store.get_component(entity.index).copied());
        if let Some(scroll) = scroll.as_mut() {
            scroll.viewport_size = UiSize::new(abs.width, abs.height);
            scroll.content_size = UiSize::new(content_width, content_height);
            scroll.clamp();
            if let Some(store) = world.get_component_store_mut::<UiScroll>() {
                store.get_mut_store().attach(entity.index, *scroll);
            }
        }
        let scroll_clip = scroll.map(|_| clip_to_parent(abs, parent_clip));
        // Open dropdowns paint their option list outside their own bounds, so
        // they ignore floating-window clipping and keep the clip they had
        // before entering the window. Scroll containers still win because
        // their viewport is an explicit content boundary.
        let clip = match scroll_clip {
            Some(scroll_clip) => Some(scroll_clip),
            None if is_open_dropdown => overflow_clip,
            None => parent_clip,
        };
        // A floating window never clips its own paint, but all of its
        // descendants stay inside the window bounds.
        let child_clip = match scroll_clip {
            Some(scroll_clip) => Some(scroll_clip),
            None if is_floating_window => Some(clip_to_parent(abs, parent_clip)),
            None => clip,
        };
        let child_overflow_clip = match scroll_clip {
            Some(scroll_clip) => Some(scroll_clip),
            None => overflow_clip,
        };
        let visible = parent_visible
            && world
                .get_component_store::<UiStyle>()
                .and_then(|store| {
                    store
                        .get_component(entity.index)
                        .and_then(|style| style.visible)
                })
                .unwrap_or(true);

        world
            .attach_component(
                &entity,
                UiComputed {
                    rect: abs,
                    content,
                    clip,
                    z_index,
                    visible,
                },
            )
            .ok();

        let child_origin = if let Some(scroll) = scroll {
            (abs.x - scroll.offset.x, abs.y - scroll.offset.y)
        } else {
            (abs.x, abs.y)
        };
        for (index, child) in self
            .taffy
            .children(node)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            self.write_computed(
                world,
                child,
                child_origin,
                ClipState {
                    clip: child_clip,
                    overflow: child_overflow_clip,
                },
                z_index + index as i32 + 1,
                visible,
            );
        }
    }
}

/// Pulls floating windows back into their root viewport.
///
/// Runs after a layout pass so the computed rectangles and insets agree. Windows
/// being dragged or resized are skipped; the widget marks layout dirty when the
/// interaction ends, so the final position is clamped on the next pass.
fn clamp_floating_windows(world: &mut World) -> bool {
    let entities: Vec<Entity> = world.entities().collect();
    let mut changed = false;

    for entity in entities {
        let (is_floating, interacting) = world
            .get_component_store::<crate::widgets::UiWidget>()
            .and_then(|store| {
                store.get_component(entity.index).map(|widget| {
                    (
                        matches!(
                            widget.spec,
                            crate::widgets::UiWidgetSpec::FloatingWindow { .. }
                        ),
                        widget.dragging || widget.resizing,
                    )
                })
            })
            .unwrap_or((false, false));
        if !is_floating || interacting {
            continue;
        }

        let Some(current) = world
            .get_component_store::<UiComputed>()
            .and_then(|store| store.get_component(entity.index).copied())
        else {
            continue;
        };
        if !current.visible {
            continue;
        }
        let Some(mut layout) = world
            .get_component_store::<UiLayout>()
            .and_then(|store| store.get_component(entity.index).cloned())
        else {
            continue;
        };

        let parent = world.get_component_store::<UiNode>().and_then(|store| {
            store
                .get_component(entity.index)
                .and_then(|node| node.parent)
        });
        let mut root = parent.unwrap_or(entity);
        while let Some(next) = world
            .get_component_store::<UiNode>()
            .and_then(|store| store.get_component(root.index).and_then(|node| node.parent))
        {
            root = next;
        }

        let Some(view) = world
            .get_component_store::<UiView>()
            .and_then(|store| store.get_component(root.index).copied())
        else {
            continue;
        };
        if !view.visible {
            continue;
        }
        let viewport = UiRect::new(
            view.offset.x,
            view.offset.y,
            view.size.width,
            view.size.height,
        );
        let containing_block = match parent {
            Some(parent) if parent != root => world
                .get_component_store::<UiComputed>()
                .and_then(|store| {
                    store
                        .get_component(parent.index)
                        .map(|computed| computed.rect)
                })
                .unwrap_or(viewport),
            _ => viewport,
        };

        if clamp_floating_window(&mut layout, current.rect, viewport, containing_block) {
            let _ = world.attach_component(&entity, layout);
            changed = true;
        }
    }

    changed
}

/// ECS resource wrapper for [`UiLayoutEngine`].
pub struct UiLayoutState {
    engine: Mutex<UiLayoutEngine>,
}

impl Default for UiLayoutState {
    fn default() -> Self {
        Self {
            engine: Mutex::new(UiLayoutEngine::default()),
        }
    }
}

impl UiLayoutState {
    pub fn with<R>(&self, f: impl FnOnce(&mut UiLayoutEngine) -> R) -> R {
        let mut engine = self.engine.lock().expect("ui layout mutex poisoned");
        f(&mut engine)
    }

    pub fn compute(&self, world: &mut World, measure: &mut dyn UiTextMeasure) -> bool {
        self.with(|engine| engine.compute(world, measure))
    }

    pub fn needs_layout(&self, world: &World) -> bool {
        self.with(|engine| engine.needs_layout(world))
    }
}

pub struct UiReconcileState {
    pub frames_since_full: u64,
    pub periodic_full_interval: u64,
}

impl Default for UiReconcileState {
    fn default() -> Self {
        Self {
            frames_since_full: 0,
            periodic_full_interval: 30,
        }
    }
}

fn text_style(world: &World, entity: Entity, themes: Option<&UiThemes>) -> UiTextStyle {
    if let Some(style) = world
        .get_component_store::<UiResolvedStyle>()
        .and_then(|store| {
            store
                .get_component(entity.index)
                .and_then(|style| style.text.clone())
        })
    {
        return UiTextStyle {
            size: style.size,
            line_height: style.line_height,
            color: crate::style::UiColor::Rgba(style.color),
            family: style.family.clone(),
            weight: style.weight,
            wrap: style.wrap,
            ellipsis: style.ellipsis,
        };
    }
    if let Some(style) = world.get_component_store::<UiStyle>().and_then(|store| {
        store
            .get_component(entity.index)
            .and_then(|style| style.text.clone())
    }) {
        return style;
    }
    themes
        .and_then(|themes| themes.get(None).typography.get("body").cloned())
        .unwrap_or_default()
}

fn theme_layout_revision(world: &World) -> u64 {
    world
        .get_resource::<UiThemes>()
        .map(|themes| {
            themes
                .named
                .values()
                .map(|theme| theme.layout_revision)
                .chain(std::iter::once(themes.active.layout_revision))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn any_layout_dirty(world: &World) -> bool {
    let Some(store) = world.get_component_store::<UiDirty>() else {
        return false;
    };
    store
        .components
        .iter()
        .any(|dirty| dirty.contains(UiDirty::LAYOUT) || dirty.contains(UiDirty::HIERARCHY))
}

fn clear_layout_dirty(world: &mut World) {
    let entities: Vec<Entity> = world.entities().collect();
    if let Some(store) = world.get_component_store_mut::<UiDirty>() {
        let store = store.get_mut_store();
        for entity in entities {
            if let Some(dirty) = store.get_component_mut(entity.index) {
                dirty.remove(UiDirty::LAYOUT | UiDirty::HIERARCHY);
            }
        }
    }
}

fn resolved_dimension(value: UiDimension, parent: f32) -> f32 {
    match value {
        UiDimension::Length(value) => value,
        UiDimension::Percent(value) => parent * value / 100.0,
        UiDimension::Auto => 0.0,
    }
}
