# Migrating from iced UI

v1.6 is the last release that supports iced. The v1.7 stack is ECS-native and
document-first. This note maps the common iced concepts to the new crates.

| iced concept | Ignit v1.7 |
|---|---|
| `Element` tree | `UiDocumentNode` / `UiBuilder` / RON `UiDocument` |
| Widget state in the view function | `UiWidget` components plus semantic `UiEvent`/`UiAction` |
| `Message` enum | `UiEvent::Action` with a named `UiAction` |
| `Command`/`Subscription` | ECS systems and `Events<UiEvent>` |
| `Theme`/palette | `UiTheme` tokens, `UiStyleClass`, `UiThemeScope` |
| `Container`/`Row`/`Column` | `UiLayout` flex/block/grid on any node |
| `scrollable` | `UiScroll` + `UiScrollContainer` and a clipped scroll view |
| `text_input` | `UiWidgetSpec::TextBox` + `TextEditState` in `ignit_ui_text` |
| iced overlay renderer | `ignit_ui_render::UiLayerRenderer` at `RenderLayer::UI` |
| `window::Event` queue | generic `WindowEventQueue`; iced queue is transition-only |
| Cursor grab workarounds | `InputCapture` + runtime capture masking |

## Debug panels

The Performance, ECS, Console, Rendering, and Animation panels are still
implemented on iced in this branch. Porting them is #1222, which is intentionally
out of scope for this change. Removing iced (#1223) is blocked until that port
lands; do not add another iced feature or widget in the meantime.

The new UI can coexist with the iced debug overlay: rendering uses separate
layer renderers at `RenderLayer::UI`, and input capture is the union of
`IcedCaptured*` resources and the generic `InputCapture`.

## Migration sequence

1. Replace iced view functions with `UiBuilder`/RON documents and widget specs.
2. Move transient UI state into ECS components or `UiId`-keyed state.
3. Replace messages with `UiAction` names and read `Events<UiEvent>`.
4. Move colors/typography into `UiTheme` tokens and classes.
5. Move text editing to `UiWidgetSpec::TextBox` + `TextEditState`.
6. Attach `UiRenderModule` and let `UiSchedule` run the UI phase.
7. Remove iced imports only after #1222 ports the debug panels.
