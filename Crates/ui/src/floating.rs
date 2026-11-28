//! Viewport clamping for floating windows.
//!
//! A floating window is absolutely positioned, so a viewport resize would
//! otherwise leave it outside the reachable area. These helpers keep the window
//! inside the current root view and rewrite its layout so the next layout pass
//! agrees with the clamped bounds.

use crate::geometry::UiRect;
use crate::layout::{UiDimension, UiLayout};

/// Values below this are treated as layout rounding noise.
const EPSILON: f32 = 0.01;

/// Clamps `rect` inside `viewport`, shrinking it when it does not fit.
///
/// A window that already fits keeps its position. A window that is larger than
/// the viewport is reduced to the viewport size so its title bar and resize grip
/// remain reachable.
pub fn clamp_floating_rect(rect: UiRect, viewport: UiRect) -> UiRect {
    let viewport_width = viewport.width.max(0.0);
    let viewport_height = viewport.height.max(0.0);
    let width = rect.width.max(0.0).min(viewport_width);
    let height = rect.height.max(0.0).min(viewport_height);
    let max_x = viewport.x + viewport_width - width;
    let max_y = viewport.y + viewport_height - height;

    UiRect::new(
        rect.x.clamp(viewport.x, max_x.max(viewport.x)),
        rect.y.clamp(viewport.y, max_y.max(viewport.y)),
        width,
        height,
    )
}

/// Applies [`clamp_floating_rect`] to a floating window's layout.
///
/// `current` is the window's absolute computed rectangle, `viewport` is the
/// root view rectangle, and `containing_block` is the absolute rectangle the
/// window's inset is resolved against. Returns `true` when the layout changed.
pub fn clamp_floating_window(
    layout: &mut UiLayout,
    current: UiRect,
    viewport: UiRect,
    containing_block: UiRect,
) -> bool {
    let clamped = clamp_floating_rect(current, viewport);
    let mut changed = false;

    if (clamped.width - current.width).abs() > EPSILON {
        layout.width = UiDimension::Length(clamped.width);
        changed = true;
    }
    if (clamped.height - current.height).abs() > EPSILON {
        layout.height = UiDimension::Length(clamped.height);
        changed = true;
    }

    if (clamped.x - current.x).abs() > EPSILON || (clamped.width - current.width).abs() > EPSILON {
        place_horizontal(layout, clamped.x, clamped.width, containing_block);
        changed = true;
    }
    if (clamped.y - current.y).abs() > EPSILON || (clamped.height - current.height).abs() > EPSILON
    {
        place_vertical(layout, clamped.y, clamped.height, containing_block);
        changed = true;
    }

    changed
}

fn place_horizontal(layout: &mut UiLayout, x: f32, width: f32, containing_block: UiRect) {
    let anchored_to_right = matches!(layout.inset.left, UiDimension::Auto)
        && !matches!(layout.inset.right, UiDimension::Auto);
    if anchored_to_right {
        layout.inset.right =
            UiDimension::Length(containing_block.x + containing_block.width - (x + width));
    } else {
        layout.inset.left = UiDimension::Length(x - containing_block.x);
        if !matches!(layout.inset.right, UiDimension::Auto) {
            layout.inset.right = UiDimension::Auto;
        }
    }
}

fn place_vertical(layout: &mut UiLayout, y: f32, height: f32, containing_block: UiRect) {
    let anchored_to_bottom = matches!(layout.inset.top, UiDimension::Auto)
        && !matches!(layout.inset.bottom, UiDimension::Auto);
    if anchored_to_bottom {
        layout.inset.bottom =
            UiDimension::Length(containing_block.y + containing_block.height - (y + height));
    } else {
        layout.inset.top = UiDimension::Length(y - containing_block.y);
        if !matches!(layout.inset.bottom, UiDimension::Auto) {
            layout.inset.bottom = UiDimension::Auto;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> UiRect {
        UiRect::new(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn an_inside_window_keeps_its_bounds() {
        let rect = UiRect::new(120.0, 80.0, 300.0, 200.0);
        assert_eq!(clamp_floating_rect(rect, viewport()), rect);
    }

    #[test]
    fn an_offscreen_window_is_pulled_back_inside() {
        let rect = UiRect::new(1200.0, 900.0, 200.0, 150.0);
        assert_eq!(
            clamp_floating_rect(rect, viewport()),
            UiRect::new(600.0, 450.0, 200.0, 150.0)
        );
    }

    #[test]
    fn an_oversized_window_shrinks_to_fit() {
        let rect = UiRect::new(120.0, 80.0, 1200.0, 900.0);
        assert_eq!(
            clamp_floating_rect(rect, viewport()),
            UiRect::new(0.0, 0.0, 800.0, 600.0)
        );
    }

    #[test]
    fn negative_positions_are_pulled_back_to_the_origin() {
        let rect = UiRect::new(-300.0, -200.0, 200.0, 150.0);
        assert_eq!(
            clamp_floating_rect(rect, viewport()),
            UiRect::new(0.0, 0.0, 200.0, 150.0)
        );
    }

    #[test]
    fn layout_inset_is_rewritten_to_the_clamped_position() {
        let mut layout = UiLayout {
            inset: crate::layout::UiEdges {
                left: UiDimension::Length(1200.0),
                right: UiDimension::Auto,
                top: UiDimension::Length(24.0),
                bottom: UiDimension::Auto,
            },
            ..Default::default()
        };
        let current = UiRect::new(1200.0, 24.0, 200.0, 150.0);

        assert!(clamp_floating_window(
            &mut layout,
            current,
            viewport(),
            viewport()
        ));
        assert_eq!(layout.inset.left, UiDimension::Length(600.0));
        assert_eq!(layout.inset.top, UiDimension::Length(24.0));
    }

    #[test]
    fn shrink_to_fit_rewrites_the_layout_size() {
        let mut layout = UiLayout {
            width: UiDimension::Length(1200.0),
            height: UiDimension::Length(900.0),
            inset: crate::layout::UiEdges {
                left: UiDimension::Length(0.0),
                right: UiDimension::Auto,
                top: UiDimension::Length(0.0),
                bottom: UiDimension::Auto,
            },
            ..Default::default()
        };
        let current = UiRect::new(0.0, 0.0, 1200.0, 900.0);

        assert!(clamp_floating_window(
            &mut layout,
            current,
            viewport(),
            viewport()
        ));
        assert_eq!(layout.width, UiDimension::Length(800.0));
        assert_eq!(layout.height, UiDimension::Length(600.0));
    }
}
