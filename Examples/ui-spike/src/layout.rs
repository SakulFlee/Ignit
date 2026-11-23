//! Taffy layout tree for the spike.

use taffy::prelude::*;

use crate::text::TextEngine;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.w && y <= self.y + self.h
    }

    pub fn intersect(&self, other: Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.w).min(other.x + other.w);
        let bottom = (self.y + self.h).min(other.y + other.h);
        (right > x && bottom > y).then_some(Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        })
    }
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    Root,
    Panel {
        fill: [f32; 4],
        radius: f32,
    },
    Label {
        text: String,
        size: f32,
        color: [f32; 4],
    },
    Button {
        label: String,
        on: bool,
    },
    Image,
    Scroll,
    Spacer,
}

#[derive(Clone, Debug)]
pub struct NodeData {
    pub id: &'static str,
    pub kind: NodeKind,
    pub interactive: bool,
}

#[derive(Clone, Debug)]
pub struct PlacedNode {
    pub id: &'static str,
    pub kind: NodeKind,
    pub rect: Rect,
    pub content: Rect,
    pub clip: Option<Rect>,
    pub interactive: bool,
}

pub struct UiTree {
    pub taffy: TaffyTree<NodeData>,
    pub root: NodeId,
}

// Taffy 0.15's `CompactLength` stores a `*const ()` for custom lengths even
// when only static/default lengths are used, so `TaffyTree` is not `Send` or
// `Sync`. The spike keeps the tree on the render thread and never shares it;
// the real core must either constrain the style type or hold layout state in
// a single-threaded owner. This is recorded as a spike finding.
unsafe impl Send for UiTree {}
unsafe impl Sync for UiTree {}

fn padding(value: f32) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        left: length(value),
        right: length(value),
        top: length(value),
        bottom: length(value),
    }
}

fn gap(value: f32) -> Size<LengthPercentage> {
    Size {
        width: length(value),
        height: length(value),
    }
}

fn column(gap_px: f32, pad: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::STRETCH,
        gap: gap(gap_px),
        padding: padding(pad),
        ..Default::default()
    }
}

fn row(gap_px: f32, pad: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::CENTER,
        gap: gap(gap_px),
        padding: padding(pad),
        ..Default::default()
    }
}

fn leaf() -> Style {
    Style {
        flex_shrink: 0.0,
        ..Default::default()
    }
}

impl UiTree {
    /// Wide/tall synthetic tree used by the layout benchmark.
    pub fn synthetic(count: usize, width: f32, height: f32) -> Self {
        let mut taffy: TaffyTree<NodeData> = TaffyTree::new();
        let mut rows = Vec::with_capacity(count);
        for index in 0..count {
            let text = format!("Benchmark row {index:04} with a little wrapped copy");
            rows.push(
                taffy
                    .new_leaf_with_context(
                        leaf(),
                        NodeData {
                            id: "bench-row",
                            kind: NodeKind::Label {
                                text,
                                size: 14.0,
                                color: [1.0; 4],
                            },
                            interactive: false,
                        },
                    )
                    .unwrap(),
            );
        }
        let column_node = taffy
            .new_with_children(
                Style {
                    flex_shrink: 0.0,
                    ..column(4.0, 0.0)
                },
                &rows,
            )
            .unwrap();
        taffy
            .set_node_context(
                column_node,
                Some(NodeData {
                    id: "bench-column",
                    kind: NodeKind::Spacer,
                    interactive: false,
                }),
            )
            .unwrap();
        let root = taffy
            .new_with_children(
                Style {
                    size: Size {
                        width: length(width),
                        height: length(height),
                    },
                    ..column(4.0, 8.0)
                },
                &[column_node],
            )
            .unwrap();
        taffy
            .set_node_context(
                root,
                Some(NodeData {
                    id: "bench-root",
                    kind: NodeKind::Root,
                    interactive: false,
                }),
            )
            .unwrap();
        Self { taffy, root }
    }

    pub fn demo(width: f32, height: f32, scale: f32, status: &str) -> Self {
        let s = scale.max(0.5);
        let mut taffy: TaffyTree<NodeData> = TaffyTree::new();

        let title = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "title",
                    kind: NodeKind::Label {
                        text: "Ignit UI spike".into(),
                        size: 28.0 * s,
                        color: [0.96, 0.97, 1.0, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();
        let status_node = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "status",
                    kind: NodeKind::Label {
                        text: status.into(),
                        size: 14.0 * s,
                        color: [0.55, 0.85, 0.70, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();
        let spacer = taffy
            .new_leaf_with_context(
                Style {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                NodeData {
                    id: "header-spacer",
                    kind: NodeKind::Spacer,
                    interactive: false,
                },
            )
            .unwrap();
        let header = taffy
            .new_with_children(row(12.0 * s, 0.0), &[title, spacer, status_node])
            .unwrap();
        taffy
            .set_node_context(
                header,
                Some(NodeData {
                    id: "header",
                    kind: NodeKind::Spacer,
                    interactive: false,
                }),
            )
            .unwrap();

        let left_title = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "left-title",
                    kind: NodeKind::Label {
                        text: "Left panel".into(),
                        size: 18.0 * s,
                        color: [0.90, 0.93, 1.0, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();
        let left_copy = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "left-copy",
                    kind: NodeKind::Label {
                        text: "Taffy measures this wrapped paragraph through cosmic-text. \
                               Resize the window and the line breaks should follow the available width \
                               without re-rasterizing unchanged glyphs."
                            .into(),
                        size: 14.0 * s,
                        color: [0.72, 0.76, 0.86, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();
        let button = taffy
            .new_leaf_with_context(
                Style {
                    size: Size {
                        width: length(180.0 * s),
                        height: length(44.0 * s),
                    },
                    ..leaf()
                },
                NodeData {
                    id: "toggle",
                    kind: NodeKind::Button {
                        label: "Toggle state".into(),
                        on: status.contains("on"),
                    },
                    interactive: true,
                },
            )
            .unwrap();
        let image = taffy
            .new_leaf_with_context(
                Style {
                    size: Size {
                        width: length(96.0 * s),
                        height: length(96.0 * s),
                    },
                    ..leaf()
                },
                NodeData {
                    id: "image",
                    kind: NodeKind::Image,
                    interactive: false,
                },
            )
            .unwrap();
        let left = taffy
            .new_with_children(
                Style {
                    size: Size {
                        width: length(320.0 * s),
                        height: auto(),
                    },
                    flex_shrink: 0.0,
                    ..column(12.0 * s, 16.0 * s)
                },
                &[left_title, left_copy, button, image],
            )
            .unwrap();
        taffy
            .set_node_context(
                left,
                Some(NodeData {
                    id: "left",
                    kind: NodeKind::Panel {
                        fill: [0.105, 0.125, 0.175, 1.0],
                        radius: 12.0 * s,
                    },
                    interactive: false,
                }),
            )
            .unwrap();

        let right_title = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "right-title",
                    kind: NodeKind::Label {
                        text: "Scissored scroll view".into(),
                        size: 18.0 * s,
                        color: [0.90, 0.93, 1.0, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();

        let mut rows = Vec::new();
        for index in 0..24 {
            let row_node = taffy
                .new_leaf_with_context(
                    leaf(),
                    NodeData {
                        id: "scroll-row",
                        kind: NodeKind::Label {
                            text: format!(
                                "Row {index:02}  ·  clipping keeps this list inside its panel  ·  \
                                 cache-friendly glyph atlas"
                            ),
                            size: 14.0 * s,
                            color: [0.70, 0.74, 0.84, 1.0],
                        },
                        interactive: false,
                    },
                )
                .unwrap();
            rows.push(row_node);
        }
        let rows_column = taffy
            .new_with_children(
                Style {
                    flex_shrink: 0.0,
                    ..column(8.0 * s, 0.0)
                },
                &rows,
            )
            .unwrap();
        taffy
            .set_node_context(
                rows_column,
                Some(NodeData {
                    id: "scroll-rows",
                    kind: NodeKind::Spacer,
                    interactive: false,
                }),
            )
            .unwrap();
        let scroll = taffy
            .new_with_children(
                Style {
                    size: Size {
                        width: auto(),
                        height: length(300.0 * s),
                    },
                    overflow: taffy::Point {
                        x: taffy::Overflow::Hidden,
                        y: taffy::Overflow::Hidden,
                    },
                    flex_shrink: 0.0,
                    ..column(0.0, 10.0 * s)
                },
                &[rows_column],
            )
            .unwrap();
        taffy
            .set_node_context(
                scroll,
                Some(NodeData {
                    id: "scroll",
                    kind: NodeKind::Scroll,
                    interactive: false,
                }),
            )
            .unwrap();
        let right = taffy
            .new_with_children(
                Style {
                    flex_grow: 1.0,
                    ..column(12.0 * s, 16.0 * s)
                },
                &[right_title, scroll],
            )
            .unwrap();
        taffy
            .set_node_context(
                right,
                Some(NodeData {
                    id: "right",
                    kind: NodeKind::Panel {
                        fill: [0.095, 0.115, 0.16, 1.0],
                        radius: 12.0 * s,
                    },
                    interactive: false,
                }),
            )
            .unwrap();

        let main = taffy
            .new_with_children(
                Style {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    ..row(16.0 * s, 0.0)
                },
                &[left, right],
            )
            .unwrap();
        taffy
            .set_node_context(
                main,
                Some(NodeData {
                    id: "main",
                    kind: NodeKind::Spacer,
                    interactive: false,
                }),
            )
            .unwrap();

        let footer = taffy
            .new_leaf_with_context(
                leaf(),
                NodeData {
                    id: "footer",
                    kind: NodeKind::Label {
                        text: "pointer hit testing  ·  1x/1.5x/2x text  ·  no SHADER_F16".into(),
                        size: 12.0 * s,
                        color: [0.52, 0.56, 0.66, 1.0],
                    },
                    interactive: false,
                },
            )
            .unwrap();

        let root = taffy
            .new_with_children(
                Style {
                    size: Size {
                        width: length(width),
                        height: length(height),
                    },
                    ..column(16.0 * s, 24.0 * s)
                },
                &[header, main, footer],
            )
            .unwrap();
        taffy
            .set_node_context(
                root,
                Some(NodeData {
                    id: "root",
                    kind: NodeKind::Root,
                    interactive: false,
                }),
            )
            .unwrap();

        Self { taffy, root }
    }

    pub fn compute(
        &mut self,
        text: &mut TextEngine,
        width: f32,
        height: f32,
        scroll_offset: f32,
    ) -> Vec<PlacedNode> {
        let mut root_style = self.taffy.style(self.root).unwrap().clone();
        root_style.size = Size {
            width: length(width),
            height: length(height),
        };
        self.taffy.set_style(self.root, root_style).unwrap();

        self.taffy
            .compute_layout_with_measure(
                self.root,
                Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::Definite(height),
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
                    match &context.kind {
                        NodeKind::Label {
                            text: value, size, ..
                        } => taffy::compute_leaf_layout(
                            inputs,
                            style,
                            |_, _| 0.0,
                            |known, available| {
                                let max_width = match available.width {
                                    AvailableSpace::Definite(value) => value,
                                    _ => known.width.unwrap_or(width),
                                };
                                let (w, h) = text.measure(value, *size, max_width);
                                Size {
                                    width: known.width.unwrap_or(w),
                                    height: known.height.unwrap_or(h),
                                }
                            },
                        ),
                        _ => {
                            taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |_, _| Size::ZERO)
                        }
                    }
                },
            )
            .unwrap();

        let mut out = Vec::new();
        self.walk(self.root, (0.0, 0.0), None, scroll_offset, &mut out);
        out
    }

    fn walk(
        &self,
        node: NodeId,
        origin: (f32, f32),
        parent_clip: Option<Rect>,
        scroll_offset: f32,
        out: &mut Vec<PlacedNode>,
    ) {
        let Some(data) = self.taffy.get_node_context(node) else {
            return;
        };
        let layout = self.taffy.layout(node).unwrap();
        let abs = Rect {
            x: origin.0 + layout.location.x,
            y: origin.1 + layout.location.y,
            w: layout.size.width,
            h: layout.size.height,
        };
        let content = Rect {
            x: abs.x + layout.border.left + layout.padding.left,
            y: abs.y + layout.border.top + layout.padding.top,
            w: (abs.w
                - layout.border.left
                - layout.border.right
                - layout.padding.left
                - layout.padding.right)
                .max(0.0),
            h: (abs.h
                - layout.border.top
                - layout.border.bottom
                - layout.padding.top
                - layout.padding.bottom)
                .max(0.0),
        };
        let clip = match data.kind {
            NodeKind::Scroll => Some(abs.intersect(parent_clip.unwrap_or(abs)).unwrap_or(abs)),
            _ => parent_clip,
        };
        out.push(PlacedNode {
            id: data.id,
            kind: data.kind.clone(),
            rect: abs,
            content,
            clip,
            interactive: data.interactive,
        });

        let child_offset = if matches!(data.kind, NodeKind::Scroll) {
            scroll_offset
        } else {
            0.0
        };
        for child in self.taffy.children(node).unwrap_or_default() {
            self.walk(
                child,
                (abs.x, abs.y - child_offset),
                clip,
                scroll_offset,
                out,
            );
        }
    }
}

pub fn hit_test(nodes: &[PlacedNode], x: f32, y: f32) -> Option<&PlacedNode> {
    nodes.iter().rev().find(|node| {
        node.interactive
            && node.rect.contains(x, y)
            && node.clip.is_none_or(|clip| clip.contains(x, y))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextEngine;

    #[test]
    fn demo_tree_lays_out() {
        let mut text = TextEngine::new();
        let mut tree = UiTree::demo(1200.0, 800.0, 1.0, "off");
        let nodes = tree.compute(&mut text, 1200.0, 800.0, 0.0);
        assert!(nodes.iter().any(|node| node.id == "toggle"));
        assert!(nodes.iter().any(|node| node.id == "scroll"));
        let toggle = nodes.iter().find(|node| node.id == "toggle").unwrap();
        assert!(toggle.rect.w > 100.0);
        assert!(toggle.rect.h > 30.0);
    }

    #[test]
    fn hit_test_finds_button() {
        let mut text = TextEngine::new();
        let mut tree = UiTree::demo(1200.0, 800.0, 1.0, "off");
        let nodes = tree.compute(&mut text, 1200.0, 800.0, 0.0);
        let toggle = nodes.iter().find(|node| node.id == "toggle").unwrap();
        let hit = hit_test(&nodes, toggle.rect.x + 5.0, toggle.rect.y + 5.0);
        assert_eq!(hit.map(|node| node.id), Some("toggle"));
    }
}
