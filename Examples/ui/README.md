# Ignit UI showcase

Runs the ECS-native UI without iced.

```bash
cd Examples/ui && cargo run
# or: cargo run -p template_ui
```

The screen contains code-authored panels and a RON-authored panel
(`Assets/Scenes/ui_demo.ron`), buttons, toggle, checkbox, slider, progress,
text box, dropdown, tabs, collapsible content, a scroll list, a floating
window, a modal, a sparkline, and a notification button that posts transient
toasts. The theme button changes paint tokens without relaying out.

Headless coverage lives in `Crates/ui/tests`; the visual run needs a GPU/display.
