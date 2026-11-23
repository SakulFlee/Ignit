# ignit_ui_text

Text shaping, an alpha glyph atlas, and editing for Ignit UI.

- Upstream `cosmic-text 0.19` for shaping, bidi, fallback, wrapping, and
  ellipsis.
- Bundled Inter by default; optional system-font fallback via
  `TextEngine::with_system_fonts`.
- Shelf-packed multi-page R8 alpha atlas with LRU page eviction.
- Shaped-run cache keyed by text, style, width, and scale.
- `UiTextMeasure` implementation for the Taffy layout bridge.
- `TextEditState` with caret/selection, keyboard navigation, clipboard
  copy/cut/paste, and Enter submission.
- `SystemClipboard` uses `arboard`; `UiClipboard` lets tests run headless.

```rust
use ignit_ui_text::{FontStack, TextEngine, TextEditState};

let mut engine = TextEngine::new();
let shaped = engine.shape("Hello UI", &style, 240.0);
let quads = engine.quads(&shaped);

let mut edit = TextEditState::new("Hello");
edit.insert_text(" UI");
```
