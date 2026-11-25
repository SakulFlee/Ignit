# Ignit UI authoring

The v1.7 UI is ECS-native and document-first. It replaces iced for application
UI and keeps a parseable boundary between layout, text, rendering, and runtime
input.

## Crates

| Crate | Responsibility |
|---|---|
| `ignit_ui` | Nodes, forest, Taffy layout, themes, documents, focus, input routing, events, widgets |
| `ignit_ui_text` | cosmic-text shaping, RON-independent text cache, R8 alpha atlas, editing model |
| `ignit_ui_render` | wgpu batching, scissor clipping, textures, glyphs, `UiLayerRenderer`, UI phase |
| `ignit_app` | `UiSchedule`/`UiSystem` hook and generic input capture |
| `ignit_debug_ui` | Legacy iced debug panels; still present until #1222/#1223 |

The facade re-exports them as `ignit::ui`, `ignit::ui_text`, and
`ignit::ui_render`.

## Code-authored UI

```rust
use ignit::ui::{
    UiAction, UiBuilder, UiDocumentNode, UiLayout, UiStyle, UiTextStyle, UiView,
    UiWidgetSpec,
};

let mut root = UiDocumentNode::new("hud")
    .layout(UiLayout::column().width_full().height_full().padding(16.0).gap(8.0))
    .style(UiStyle::default().background(ignit::ui::UiColor::token("bg")))
    .child(
        UiDocumentNode::new("title")
            .text("Settings")
            .style(UiStyle::default().padding_text(UiTextStyle {
                size: 24.0,
                weight: 600,
                ..Default::default()
            })),
    )
    .child(
        UiDocumentNode::new("apply")
            .layout(UiLayout::default().size(140.0, 36.0))
            .text("Apply")
            .widget(UiWidgetSpec::Button { label: "Apply".into() })
            .action(UiAction::new("apply"))
            .focusable()
            .style(UiStyle::default().class("panel")),
    );
root.view = Some(UiView::new(1280.0, 720.0));

let handles = UiBuilder::new().root(root).build(&mut world)?;
handles[0].set_text(&mut world, "Updated at runtime");
```

Widgets are ordinary nodes with a `UiWidget` component. They emit semantic
`UiEvent::ValueChanged`/`Action` values; do not store callbacks in components.

## RON documents

Documents are versioned, use stable `UiId`s, and preserve state across reloads.

```ron
(
    version: 1,
    roots: [
        (
            id: "hud",
            view: Some((size: (width: 800.0, height: 600.0), scale: 1.0, order: 0, visible: true, modal: false)),
            layout: (display: Flex, direction: Column, width: Length(800.0), height: Length(600.0), padding: (left: Length(16.0), right: Length(16.0), top: Length(16.0), bottom: Length(16.0)), gap: (width: Length(8.0), height: Length(8.0))),
            style: (background: Some(Token("bg"))),
            children: [
                (id: "title", layout: (display: Flex, direction: Column, width: Auto, height: Auto), text: Some((value: "Hello"))),
            ],
        ),
    ],
)
```

Omitted struct fields use their `Default` values. `UiColor` is either
`Rgba([r, g, b, a])` or `Token("name")`.

Load with `UiDocument::from_ron`, then `spawn_document`; for assets use
`ignit_ui::load_document` and `reload_document`. Desktop hot reload compares the
asset mtime and restores scroll offsets, text values, and focus by ID.

## Themes and classes

A theme contains token maps for colors, typography, spacing, radii, borders,
shadows, and motion, plus style classes.

- Inline node styles override class base values.
- Hovered/pressed/focused/disabled state patches override both.
- `UiThemeScope` on an ancestor selects a named theme and inherits classes down
  the subtree.
- `set_color` bumps only the paint revision; layout is skipped.
- `set_spacing`, `set_typography`, and `set_class` bump the layout revision.

```rust
theme.set_color("accent", [0.3, 0.7, 1.0, 1.0]);
theme.set_spacing("lg", 20.0);
theme.set_class("danger", UiStyleClass {
    style: UiStylePatch { background: Some(UiColor::token("danger")), ..Default::default() },
    ..Default::default()
});
```

## Runtime phase

`UiRenderModule` installs both the overlay renderer and the UI update system.

Per frame:

1. window events are forwarded to `WindowEventQueue`;
2. gamepads are polled;
3. the core schedule updates time;
4. the `UiSchedule` runs with `&mut World`:
   reconcile → resolve styles → layout if dirty → hit-test/route → focus and
   keyboard navigation → widget state → draw list;
5. deferred game touch/wheel input is filtered by iced capture and generic
   `InputCapture`;
6. a focused text field clears game keyboard input;
7. `InputSnapshot` is published;
8. the game schedule runs.

Game systems therefore see same-frame `UiEvent`s and capture masks.

## Scale factor

`WindowScaleFactor` is updated from winit. Root `UiView.size` is set from the
physical window size divided by the factor, and `UiView.scale` drives the
renderer's logical-to-physical transform. Text is shaped and rasterized at the
scaled size by `ignit_ui_text`.

## Showcase

```bash
cd Examples/ui && cargo run
# or: cargo run -p template_ui
```

The example covers code and RON screens, theme switching, resize, buttons,
toggle/checkbox, slider, progress, single- and multi-line text editing with
clipboard, dropdown, tabs, collapsible, scroll list, a resizable floating
window, modal, sparkline, and notifications.

## Known limits in v1.7

- Docking, tabs in the editor sense, and 3D viewport shrink are v1.8.
- IME composition UI, accessibility, gamepad spatial navigation, and offscreen
  damage caching are out of scope.
- RichText spans, nine-slice widgets, and full list virtualization are not
  complete; the generic text/image/list paths cover the basic cases.
- `TextBox` supports multi-line editing with wrapped rows and vertical
  scrolling. IME composition and accessibility stay deferred.
- The iced debug UI still exists until #1222/#1223; do not add new iced UI.
