# ignit_ui

ECS-native retained UI core for Ignit.

This crate is deliberately renderer- and platform-agnostic. It owns:

- stable `UiId` nodes and a multi-root `UiForest`;
- serializable `UiLayout` styles bridged to Taffy 0.15;
- token themes, style classes, state overrides, and theme scopes;
- versioned RON documents, spawning, hot reload, and state restoration;
- focus/tab/arrow navigation and modal scopes;
- pointer/keyboard input routing and `UiEvent`s;
- explicit `UiDirty` flags with periodic full reconciliation;
- a renderer-agnostic `UiDrawList` consumed by `ignit_ui_render`.

The text stack lives in `ignit_ui_text` and implements `UiTextMeasure`. The
runtime phase lives in `ignit_app` integration and feeds `UiInputState`.

## Code-authored UI

```rust
use ignit_ui::{UiBuilder, UiDocumentNode, UiLayout, UiStyle, UiTextStyle, UiView};

let mut root = UiDocumentNode::new("hud")
    .layout(UiLayout::column().size(800.0, 600.0).padding(16.0).gap(8.0))
    .style(UiStyle::default().background(ignit_ui::UiColor::token("bg")))
    .child(
        UiDocumentNode::new("title")
            .layout(UiLayout::default())
            .text("Hello")
            .style(UiStyle::default().padding_text(UiTextStyle {
                size: 22.0,
                ..Default::default()
            })),
    );
root.view = Some(UiView::new(800.0, 600.0));
let handles = UiBuilder::new().root(root).build(&mut world)?;
handles[0].set_text(&mut world, "Updated");
```

## RON documents

Documents are versioned and use stable IDs:

```ron
(
  version: 1,
  roots: [
    (
      id: "hud",
      layout: (display: Flex, direction: Column, width: Percent(100.0), height: Percent(100.0)),
      style: (background: Some(Token("bg")), classes: ["panel"]),
      children: [
        (id: "title", text: Some((value: "Hello")), layout: (height: Auto)),
      ],
    ),
  ],
)
```

Use [`load_document`] and [`reload_document`] with `ignit_file_manager` for
desktop hot reload. Reload preserves scroll, text values, and focus by `UiId`.

## Frame contract

`ignit_app` calls `init` once and then `update` in the pre-game UI phase:

1. resolve styles;
2. layout only when dirty or the periodic reconcile fires;
3. hit test and route pointer input;
4. update focus and keyboard navigation;
5. build `UiDrawList` and publish `UiInputCapture`/`UiEvent`s.

Game systems then see the same-frame capture state and event queue.

## Notifications

`ignit_ui::UiNotifications` is a screen-space toast queue painted after the
normal z sort, so cards sit above every root without joining layout or input
routing. `ignit_ui_render` advances the queue from `DeltaTime` every frame.

```rust
use ignit_ui::{UiNotification, UiNotifications};

if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
    notifications.post(UiNotification::warning("GPU pressure", "Quality reduced"));
}
```

Keyed notifications coalesce while visible and expose a repeat count:

```rust
notifications.post(
    UiNotification::warning("GPU pressure", "Quality reduced")
        .key("gpu.pressure")
        .sticky(),
);
```

Cards have a colored header bar labelled `INFO`, `SUCCESS`, `WARNING`,
`CRITICAL`, or `DEBUG`, using the severity tokens (`info`, `success`,
`warning`, `danger`, `debug`) with auto-contrast text and a short slide/fade-in.
`UiNotificationPlacement` covers the four cardinals and four intercardinals;
the default is the top-right corner. `next()` cycles through them for settings
UI.

`max_visible` (default 6) bounds the painted stack and `max_queued` (default
32) bounds the total queue. Durations accumulate only while a card has a
painted slot. Cards pushed below the stack pause their timer and resume when
promoted, so nothing expires unread just because it was queued.

## Notes

- Taffy 0.15's `TaffyTree` contains a tagged pointer and is not `Send`/`Sync`.
  The engine is stored in a `Mutex` and only asserts `Send`; it never leaves
  the UI thread. Do not remove that invariant without re-checking Taffy.
- The ECS has no change detection. External writers must set `UiDirty` or rely
  on the periodic full reconcile.
- Text shaping, glyph atlases, and the widget set are separate crates/issues.

## Widgets

`ignit_ui::widgets` provides the v1 control set as ECS components:

- `Button`, `Toggle`, `Checkbox`, `Slider`, `ProgressBar`, `TextBox`,
  `Dropdown`, `ListBox`, `Tabs`, `Collapsible`, `FloatingWindow`,
  `Sparkline`, `Tooltip`, `Popup`, `Menu`, `Modal`, `Divider`.
- `TextBox` supports keyboard and mouse selection, highlighted ranges, and
  Ctrl+C/X/V through `UiClipboardResource`. The core defaults to an in-memory
  clipboard; `ignit_ui_render` installs the system clipboard.
- `FloatingWindow` drags from the title bar, resizes from the bottom-right
  grip, and rises above other floating windows when clicked.
- Widgets emit semantic `UiEvent::ValueChanged`/`Action` values; no stored
  callbacks.
- `Examples/ui` showcases the code-authored and RON-authored screens.
