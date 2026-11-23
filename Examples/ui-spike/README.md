# UI foundation spike (not shipped)

Temporary proof for [#1215](https://forgejo.sakul-flee.de/SakulFlee/Ignit/issues/1215). It is
intentionally small and does not define the final APIs. The branch is not
intended to merge; the findings below feed #1216/#1218/#1219.

## Run

```bash
cargo run -p ui_spike
```

Desktop only. The example draws a Taffy-laid-out panel with:

- wrapped cosmic-text labels at 1x/1.5x/2x scale (scale comes from the window
  scale factor);
- a shelf-packed R8 alpha glyph atlas rasterized with swash;
- rounded rectangles, a soft shadow, a textured image, and text;
- a scroll view whose children are clipped with `set_scissor_rect`;
- pointer hover and click hit testing for the toggle button.

## Headless checks

```bash
cargo test -p ui_spike --test layout -- --nocapture
cargo test -p ui_spike --release --test layout -- --nocapture
```

The test suite covers the 500-node layout budget, glyph/measurement cache hits,
scissor intersection, and hit testing.

## Findings

### Layout

- Taffy 0.15 lays out the 500-node synthetic tree in **0.166 ms** in the dev
  profile and **0.846 ms** in the release profile on this machine. Both are
  under the 1 ms budget, though the release number is close enough that the
  real core should keep the benchmark in CI.
- Taffy does **not** clip children for `overflow: hidden`; it only changes
  sizing/scroll-region behavior. UI clipping must be enforced by the draw list
  and scissor stack. That is fine for the v1.7 direct-overlay design.
- `compute_layout_with_measure` closures receive `(LayoutInput, NodeId,
  Option<&mut Context>, &Style)` and must return `LayoutOutput`. Use
  `taffy::compute_leaf_layout` for text instead of returning a bare `Size`.
- Containers created with `new_with_children` have no node context. The spike
  sets context on every node; a real core should keep a `NodeId -> UiNode`
  map instead of relying on Taffy's optional context slot.
- **Taffy 0.15's `TaffyTree` is neither `Send` nor `Sync`**: `CompactLength`
  stores numeric lengths in a tagged `*const ()`. The spike uses a documented
  `unsafe impl Send + Sync` because it never leaves the render thread. The real
  core should keep the Taffy tree in a single-owner wrapper and only expose
  `Send` (inside a `Mutex`) or pin layout to one thread. Revisit this with
  Taffy 0.16 before freezing the core API. This is the main design risk for
  storing layout state in an ECS resource.

### Text

- Upstream `cosmic-text 0.19` with `default-features = false, features =
  ["std", "swash"]` builds without system fontconfig. A bundled Inter face is
  enough; set the generic sans-serif family to the loaded face name.
- `Buffer::shape_until_scroll` must run before reading `layout_runs` when using
  `Buffer` directly. `glyph.physical((0.0, run.line_y), 1.0)` plus
  `image.placement.left/top` gives the correct pixel rectangle.
- The glyph atlas cache is stable: repeated shaping of the same text does not
  rasterize new glyphs, and repeated measurement hits the measurement cache.
- The spike still creates a `Buffer` per shape/measure call. The real text
  crate must cache buffers and shaped runs to satisfy the steady-state
  allocation target.
- Alpha (`Content::Mask`) glyphs cover Inter. Color/emoji glyphs need an RGBA
  atlas or a separate path later.

### Rendering

- One pipeline can cover SDF rounded rects, R8 glyphs, and RGBA textures by
  sampling both textures before the mode branch. No `SHADER_F16` is used.
- `set_scissor_rect` per batch is sufficient for rectangular clipping; no
  offscreen UI target or damage cache is needed for v1.7.
- The surface format must come from `SurfaceFormatResource`; the pipeline
  target format must match the final view.
- Persistent vertex buffers plus `queue.write_buffer` work; the real renderer
  should grow geometrically and avoid per-frame allocations.

### Input / runtime

- `CursorPosition` and `InputSnapshot` are enough for a first hover/click
  spike, but they are not the final capture model. The real runtime phase needs
  `InputCapture` before the game snapshot is published.
- `IcedEventQueue.scale_factor` is currently the only convenient scale source.
  #1221 should add `WindowScaleFactor` and keep both paths during the
  transition.
- The current `InputSnapshot` is published before the game schedule. The real
  UI phase must run before that snapshot (or the snapshot must move after the
  UI phase) for same-frame text/drag capture. The spike only proves hit
  testing, not capture.

## Environment note

This container has no display, no `/dev/dri`, and no Vulkan ICD, so the app was
built and the headless tests were run here, but the visual run needs a desktop
with a GPU.
