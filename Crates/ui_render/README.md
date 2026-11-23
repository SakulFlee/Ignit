# ignit_ui_render

wgpu renderer for `ignit_ui::UiDrawList`.

- One overlay layer at `RenderLayer::UI` recorded into the main scene encoder.
- Batched rounded rects, borders, approximate shadows, textures, lines, and
  cosmic-text glyphs.
- CPU clip stack mapped to `set_scissor_rect` per batch.
- R8 multi-page glyph atlas uploads.
- Named image textures and render-target views through `register_texture`.
- Per-root scale/offset transforms.
- Persistent, geometrically growing vertex buffers.
- No `SHADER_F16`; no offscreen UI target in v1.7.

```rust
let text = Arc::new(Mutex::new(TextEngine::new()));
let layer = UiLayerRenderer::new(&device, &queue, format, text);
app.add_module(UiRenderModule::new(layer));
```
