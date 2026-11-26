//! New-UI debug overlay, ported panel by panel from the iced implementation.
//!
//! This module is opt-in while the port is in progress: add
//! [`DebugUiNextModule`] after `UiRenderModule`, then press its toggle key.
//! The existing iced-backed [`crate::DebugUiModule`] stays available until the
//! port is complete and iced is removed.

use std::time::{Duration, Instant};

use ignit_app::{Module, UiSystem};
use ignit_ecs::{Entity, IntoSystem, System, World};
use ignit_ecs_bridge::{
    DebugSelectedEntity, FpsStats, FrameTimings, PerformanceLogConfig, TimingsHistoryConfig,
};
use ignit_ui::{
    UiAction, UiBuilder, UiDirty, UiDocumentNode, UiEvent, UiInteraction, UiLayout, UiStyle,
    UiText, UiView, UiWidget, UiWidgetSpec,
};
use ignit_ui_render::UiTextResource;
use log::{debug, warn};
use winit::keyboard::KeyCode;

use crate::state::{DebugMessage, DebugUiState, PanelId};

mod console;

const REFRESH: Duration = Duration::from_millis(33);
const PANEL_ID: &str = "debug_next_perf_window";
const ROOT_ID: &str = "debug_next_root";

/// New-UI debug overlay. Add after `UiRenderModule`.
pub struct DebugUiNextModule {
    toggle_key: KeyCode,
    allow_in_release: bool,
}

impl Default for DebugUiNextModule {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugUiNextModule {
    pub fn new() -> Self {
        Self {
            toggle_key: KeyCode::F1,
            allow_in_release: false,
        }
    }

    pub fn with_toggle_key(mut self, key: KeyCode) -> Self {
        self.toggle_key = key;
        self
    }

    pub fn allow_in_release(mut self) -> Self {
        self.allow_in_release = true;
        self
    }

    fn enabled(&self) -> bool {
        self.allow_in_release || cfg!(debug_assertions)
    }
}

#[derive(Clone, Copy)]
struct DebugUiNextRoot {
    root: Entity,
    panel: Entity,
}

impl Module for DebugUiNextModule {
    fn setup(
        &self,
        ecs: &mut World,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Vec<Box<dyn System>> {
        if !self.enabled() {
            return Vec::new();
        }
        if ecs.get_resource::<UiTextResource>().is_none() {
            warn!("DebugUiNextModule requires UiRenderModule; debug UI stays hidden");
            return Vec::new();
        }

        ecs.insert_resource(DebugUiState::new(self.toggle_key));
        ecs.insert_resource(DebugSelectedEntity::default());
        if ecs.get_resource::<TimingsHistoryConfig>().is_none() {
            ecs.insert_resource(TimingsHistoryConfig::default());
        }

        let Some((root, panel)) = build_document(ecs) else {
            warn!("DebugUiNextModule: failed to build the debug document");
            return Vec::new();
        };
        ecs.insert_resource(DebugUiNextRoot { root, panel });

        vec![
            crate::sys_debug_ui_toggle.into_system(),
            Box::new(crate::DebugUiCursorSync::new()),
            crate::sys_sync_timings_history.into_system(),
        ]
    }

    fn ui_systems(&self, ecs: &mut World) -> Vec<Box<dyn UiSystem>> {
        if !self.enabled() {
            return Vec::new();
        }
        let Some(root) = ecs.get_resource::<DebugUiNextRoot>().map(|root| *root) else {
            return Vec::new();
        };
        vec![Box::new(DebugUiNextSystem {
            state: DebugUiNextRoot {
                root: root.root,
                panel: root.panel,
            },
            last_refresh: Instant::now() - REFRESH,
            last_visible: None,
        })]
    }
}

struct DebugUiNextSystem {
    state: DebugUiNextRoot,
    last_refresh: Instant,
    last_visible: Option<bool>,
}

impl UiSystem for DebugUiNextSystem {
    fn name(&self) -> &str {
        "ignit-debug-ui-next"
    }

    fn run(&mut self, world: &mut World) {
        let visible = world
            .get_resource::<DebugUiState>()
            .is_some_and(|state| state.visible);
        if self.last_visible != Some(visible) {
            self.last_visible = Some(visible);
            set_visible(world, self.state.root, visible);
            debug!("debug UI next {}", if visible { "shown" } else { "hidden" });
        }
        if !visible {
            return;
        }

        handle_events(world, self.state.panel);
        if self.last_refresh.elapsed() < REFRESH {
            return;
        }
        self.last_refresh = Instant::now();
        refresh_performance(world);
        console::refresh(world);
    }
}

fn handle_events(world: &mut World, panel: Entity) {
    let events: Vec<UiEvent> = world
        .get_resource::<ignit_ecs::Events<UiEvent>>()
        .map(|events| events.read().cloned().collect())
        .unwrap_or_default();
    for event in events {
        match event {
            UiEvent::ValueChanged { id, value, .. } if id.as_str() == "debug_perf_console" => {
                if let Ok(on) = value.parse::<bool>() {
                    crate::handle_debug_message_world(
                        DebugMessage::SetPerformanceLogging(on),
                        world,
                    );
                }
            }
            UiEvent::ValueChanged { id, value, .. } if id.as_str() == "debug_perf_csv" => {
                if let Ok(on) = value.parse::<bool>() {
                    crate::handle_debug_message_world(DebugMessage::SetPerformanceCsv(on), world);
                }
            }
            UiEvent::Action { action, .. } if action.name == "debug_perf_hide" => {
                crate::handle_debug_message_world(
                    DebugMessage::HidePanel(PanelId::Performance),
                    world,
                );
                let _ = panel;
            }
            UiEvent::Action { action, .. } if action.name == "debug_console_clear" => {
                crate::handle_debug_message_world(DebugMessage::ClearLog, world);
            }
            UiEvent::Action { action, .. } if action.name == "debug_console_hide" => {
                crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Console), world);
            }
            UiEvent::Action { action, .. } if action.name.starts_with("debug_console_level_") => {
                let index = action
                    .name
                    .rsplit('_')
                    .next()
                    .and_then(|value| value.parse::<usize>().ok());
                if let Some(index) = index
                    && let Some((_, level)) = console::levels().get(index)
                {
                    crate::handle_debug_message_world(DebugMessage::SetMinLevel(*level), world);
                }
            }
            _ => {}
        }
    }
}

fn build_document(world: &mut World) -> Option<(Entity, Entity)> {
    let mut content = UiDocumentNode::new("debug_next_perf_scroll")
        .layout(
            UiLayout::column()
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: ignit_ui::UiDimension::Length(10.0),
                    right: ignit_ui::UiDimension::Length(10.0),
                    top: ignit_ui::UiDimension::Length(34.0),
                    bottom: ignit_ui::UiDimension::Length(10.0),
                })
                .gap(6.0),
        )
        .scroll();

    content = content
        .child(UiDocumentNode::new("debug_perf_fps").text("FPS: --").style(
            UiStyle::default().padding_text(ignit_ui::UiTextStyle {
                size: 22.0,
                weight: 600,
                ..Default::default()
            }),
        ))
        .child(UiDocumentNode::new("debug_perf_frame").text("Frame: -- ms"))
        .child(
            UiDocumentNode::new("debug_perf_graph")
                .layout(UiLayout::default().width_full().height(90.0).shrink(0.0))
                .widget(UiWidgetSpec::Sparkline {
                    values: Vec::new(),
                    max: 1.0,
                }),
        )
        .child(
            UiDocumentNode::new("debug_perf_stage_header")
                .layout(
                    UiLayout::row()
                        .width_full()
                        .height(16.0)
                        .gap(4.0)
                        .shrink(0.0),
                )
                .child(
                    UiDocumentNode::new("debug_perf_stage_header_name")
                        .layout(UiLayout::default().width_percent(38.0))
                        .text("Stage")
                        .style(small_style(true)),
                )
                .child(
                    UiDocumentNode::new("debug_perf_stage_header_values")
                        .layout(UiLayout {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .text("last      avg      max")
                        .style(small_style(true)),
                ),
        );

    for index in 0..ignit_ecs_bridge::TIMING_STAGES.len() {
        content = content.child(
            UiDocumentNode::new(format!("debug_perf_stage_row_{index}"))
                .layout(
                    UiLayout::row()
                        .width_full()
                        .height(16.0)
                        .gap(4.0)
                        .shrink(0.0),
                )
                .child(
                    UiDocumentNode::new(format!("debug_perf_stage_name_{index}"))
                        .layout(UiLayout::default().width_percent(38.0))
                        .text("--")
                        .style(small_style(true)),
                )
                .child(
                    UiDocumentNode::new(format!("debug_perf_stage_{index}"))
                        .layout(UiLayout {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .text("--")
                        .style(small_style(false)),
                ),
        );
    }

    content = content
        .child(UiDocumentNode::new("debug_perf_divider").widget(UiWidgetSpec::Divider))
        .child(
            UiDocumentNode::new("debug_perf_console_row")
                .layout(UiLayout::row().height(28.0).gap(8.0))
                .child(
                    UiDocumentNode::new("debug_perf_console_label")
                        .layout(UiLayout {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .text("Log to console"),
                )
                .child(
                    UiDocumentNode::new("debug_perf_console")
                        .layout(UiLayout::default().size(40.0, 22.0))
                        .widget(UiWidgetSpec::Toggle { on: false }),
                ),
        )
        .child(
            UiDocumentNode::new("debug_perf_csv_row")
                .layout(UiLayout::row().height(28.0).gap(8.0))
                .child(
                    UiDocumentNode::new("debug_perf_csv_label")
                        .layout(UiLayout {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .text("CSV capture"),
                )
                .child(
                    UiDocumentNode::new("debug_perf_csv")
                        .layout(UiLayout::default().size(40.0, 22.0))
                        .widget(UiWidgetSpec::Toggle { on: false }),
                ),
        );

    let panel = UiDocumentNode::new(PANEL_ID)
        .layout(
            UiLayout::column()
                .width(440.0)
                .height(420.0)
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: ignit_ui::UiDimension::Length(40.0),
                    right: ignit_ui::UiDimension::Auto,
                    top: ignit_ui::UiDimension::Length(40.0),
                    bottom: ignit_ui::UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "Performance".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel"))
        .child(content)
        .child(close_button("debug_perf_close", "debug_perf_hide"));

    let mut root = UiDocumentNode::new(ROOT_ID)
        .layout(UiLayout::column().width_full().height_full())
        .style(UiStyle::default().visible(false))
        .child(panel)
        .child(console::build());
    root.view = Some(UiView::new(1280.0, 720.0));

    ignit_ui::init(world);
    let handles = UiBuilder::new().root(root).build(world).ok()?;
    let root = handles.first()?.entity;
    let panel = world
        .get_resource::<ignit_ui::UiForest>()?
        .find(&PANEL_ID.into())?;
    if let Some(store) = world.get_component_store_mut::<UiInteraction>() {
        store.get_mut_store().attach(
            root.index,
            UiInteraction {
                disabled: true,
                ..Default::default()
            },
        );
    }
    Some((root, panel))
}

/// Timestamps occasionally arrive as garbage after a GPU timestamp reset.
/// Keep the table readable instead of printing huge or non-finite values.
fn sane_ms(value: f64) -> Option<f32> {
    let value = value as f32;
    (value.is_finite() && value.abs() <= 10_000.0).then_some(value.max(0.0))
}

fn close_button(id: &str, action: &str) -> UiDocumentNode {
    UiDocumentNode::new(id)
        .layout(
            UiLayout::row()
                .size(20.0, 20.0)
                .justify(ignit_ui::UiJustify::Center)
                .align_items(ignit_ui::UiAlign::Center)
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: ignit_ui::UiDimension::Auto,
                    right: ignit_ui::UiDimension::Length(6.0),
                    top: ignit_ui::UiDimension::Length(5.0),
                    bottom: ignit_ui::UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::Button { label: "×".into() })
        .action(UiAction::new(action))
        .focusable()
        .style(UiStyle::default().class("panel"))
        .child(UiDocumentNode::new(format!("{id}_label")).text("×"))
}

fn small_style(muted: bool) -> UiStyle {
    UiStyle::default().padding_text(ignit_ui::UiTextStyle {
        size: 11.0,
        color: if muted {
            ignit_ui::UiColor::token("text_muted")
        } else {
            ignit_ui::UiColor::token("text")
        },
        wrap: ignit_ui::UiTextWrap::NoWrap,
        ..Default::default()
    })
}

fn refresh_performance(world: &mut World) {
    let fps = world.get_resource::<FpsStats>().map_or(0.0, |stats| {
        if stats.fps > 0 {
            stats.fps as f32
        } else if stats.cycle_delta_time > 0.0 {
            (1.0 / stats.cycle_delta_time) as f32
        } else {
            0.0
        }
    });
    let frame_ms = world
        .get_resource::<FpsStats>()
        .map_or(0.0, |stats| stats.cycle_delta_time as f32 * 1000.0);
    set_text(
        world,
        "debug_perf_fps",
        format!("FPS: {fps:.1}  ({frame_ms:.2} ms)"),
    );

    set_text(
        world,
        "debug_perf_frame",
        format!("Frame: {frame_ms:.2} ms"),
    );

    let (values, max, rows) = world.get_resource::<FrameTimings>().map_or_else(
        || (Vec::new(), 1.0, Vec::new()),
        |timings| {
            let mut values: Vec<f32> = timings
                .cdt_history()
                .iter()
                .rev()
                .filter(|value| value.is_finite() && value.abs() < 10_000.0)
                .take(120)
                .copied()
                .collect();
            values.reverse();
            let max = values.iter().copied().fold(1.0, f32::max) * 1.1;
            let rows = timings
                .samples()
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let gpu = index >= ignit_ecs_bridge::GPU_STAGE_START;
                    let (good, warn) = if gpu { (2.0, 4.0) } else { (1.0, 2.0) };
                    let (value, color) = match (
                        sane_ms(sample.last_ms),
                        sane_ms(sample.avg_ms),
                        sane_ms(sample.max_ms),
                    ) {
                        (Some(last), Some(avg), Some(max)) => (
                            format!("{last:>6.2}  {avg:>6.2}  {max:>6.2}"),
                            if last <= good {
                                [0.5, 0.9, 0.6, 1.0]
                            } else if last <= warn {
                                [1.0, 0.8, 0.4, 1.0]
                            } else {
                                [1.0, 0.5, 0.45, 1.0]
                            },
                        ),
                        _ => ("     --      --      --".to_string(), [1.0, 0.5, 0.45, 1.0]),
                    };
                    (sample.name.to_string(), value, color)
                })
                .collect();
            (values, max, rows)
        },
    );
    set_sparkline(world, "debug_perf_graph", values, max);
    for (index, (name, value, color)) in rows.into_iter().enumerate() {
        set_text(world, &format!("debug_perf_stage_name_{index}"), name);
        set_text(world, &format!("debug_perf_stage_{index}"), value);
        set_text_color(world, &format!("debug_perf_stage_{index}"), color);
    }

    let config = world
        .get_resource::<PerformanceLogConfig>()
        .map(|config| *config)
        .unwrap_or_default();
    set_toggle(world, "debug_perf_console", config.console);
    set_toggle(world, "debug_perf_csv", config.csv);

    let panel_open = world
        .get_resource::<DebugUiState>()
        .map(|state| state.is_panel_visible(PanelId::Performance))
        .unwrap_or(false);
    let root = world.get_resource::<DebugUiNextRoot>().map(|root| *root);
    if let Some(root) = root {
        set_panel_open(world, root.panel, panel_open);
    }
}

fn set_text(world: &mut World, id: &str, value: String) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let current = world
        .get_component_store::<UiText>()
        .and_then(|store| store.get_component(entity.index).cloned());
    let changed = current.is_none_or(|text| text.value != value);
    if changed {
        let _ = world.attach_component(&entity, UiText::new(value));
        mark_dirty(world, entity, UiDirty::TEXT | UiDirty::LAYOUT);
    }
}

fn set_toggle(world: &mut World, id: &str, on: bool) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Toggle { on: value } = &mut widget.spec
        && *value != on
    {
        *value = on;
        changed = true;
    }
    if changed {
        mark_dirty(world, entity, UiDirty::PAINT);
    }
}

fn set_text_color(world: &mut World, id: &str, color: [f32; 4]) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let current = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned());
    let mut style = current.unwrap_or_default();
    style.text = Some(ignit_ui::UiTextStyle {
        size: 11.0,
        color: ignit_ui::UiColor::Rgba(color),
        wrap: ignit_ui::UiTextWrap::NoWrap,
        ..style.text.unwrap_or_default()
    });
    let _ = world.attach_component(&entity, style);
    mark_dirty(world, entity, UiDirty::PAINT | UiDirty::TEXT);
}

fn set_sparkline(world: &mut World, id: &str, values: Vec<f32>, max: f32) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Sparkline { values: v, max: m } = &mut widget.spec
        && (*v != values || *m != max)
    {
        *v = values;
        *m = max;
        changed = true;
    }
    if changed {
        mark_dirty(world, entity, UiDirty::PAINT);
    }
}

fn set_panel_open(world: &mut World, panel: Entity, open: bool) {
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(panel.index)
        && let UiWidgetSpec::FloatingWindow { open: value, .. } = &mut widget.spec
        && *value != open
    {
        *value = open;
        changed = true;
    }
    if changed {
        mark_dirty(world, panel, UiDirty::LAYOUT | UiDirty::PAINT);
    }
    // Keep the style in sync as well: once a widget is display:none it no
    // longer receives update_widgets work, so the open flag alone cannot
    // bring a closed window back.
    set_visible(world, panel, open);
}

fn set_visible(world: &mut World, root: Entity, visible: bool) {
    let Some(mut style) = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(root.index).cloned())
    else {
        return;
    };
    if style.visible == Some(visible) {
        return;
    }
    style.visible = Some(visible);
    let _ = world.attach_component(&root, style);
    mark_dirty(world, root, UiDirty::LAYOUT | UiDirty::PAINT);
}

fn mark_dirty(world: &mut World, entity: Entity, dirty: UiDirty) {
    if let Some(store) = world.get_component_store_mut::<UiDirty>() {
        let store = store.get_mut_store();
        let next = store
            .get_component(entity.index)
            .copied()
            .unwrap_or_default()
            | dirty;
        store.attach(entity.index, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignit_ecs_bridge::{FpsStats, PerformanceLogConfig};

    #[test]
    fn document_builds_with_a_hidden_performance_window() {
        let mut world = World::new();
        let (root, panel) = build_document(&mut world).expect("document");
        let forest = world.get_resource::<ignit_ui::UiForest>().unwrap();
        assert!(forest.find(&ROOT_ID.into()).is_some());
        assert!(forest.find(&PANEL_ID.into()).is_some());
        assert_eq!(forest.find(&ROOT_ID.into()), Some(root));
        assert_eq!(forest.find(&PANEL_ID.into()), Some(panel));
        let style = world
            .get_component_store::<UiStyle>()
            .unwrap()
            .get_component(root.index)
            .cloned()
            .unwrap();
        assert_eq!(style.visible, Some(false));
    }

    #[test]
    fn performance_content_scrolls() {
        let mut world = World::new();
        let (root, _) = build_document(&mut world).expect("document");
        set_visible(&mut world, root, true);
        let mut text = ignit_ui::UiFallbackTextMeasure;
        ignit_ui::update(&mut world, &mut text);
        let scroll = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_next_perf_scroll".into()))
            .expect("scroll entity");
        let computed = world
            .get_component_store::<ignit_ui::UiComputed>()
            .and_then(|store| store.get_component(scroll.index).copied())
            .expect("computed");
        let state = world
            .get_component_store::<ignit_ui::UiScroll>()
            .and_then(|store| store.get_component(scroll.index).copied())
            .expect("scroll state");
        assert!(
            state.content_size.height > state.viewport_size.height,
            "content {:?} viewport {:?}",
            state.content_size,
            state.viewport_size
        );

        world.insert_resource(ignit_ui::UiInputState {
            pointer: ignit_ui::UiPoint::new(
                computed.rect.x + computed.rect.width * 0.5,
                computed.rect.y + computed.rect.height * 0.5,
            ),
            wheel: ignit_ui::UiPoint::new(0.0, 60.0),
            ..Default::default()
        });
        ignit_ui::update(&mut world, &mut text);
        ignit_ui::update(&mut world, &mut text);
        let state = world
            .get_component_store::<ignit_ui::UiScroll>()
            .and_then(|store| store.get_component(scroll.index).copied())
            .expect("scroll state");
        assert!(state.offset.y > 0.0, "offset {:?}", state.offset);
    }

    #[test]
    fn refresh_updates_fps_and_toggles() {
        let mut world = World::new();
        let _ = build_document(&mut world).expect("document");
        world.insert_resource(FpsStats {
            fps: 120,
            total_delta_time: 1.0,
            cycle_delta_time: 0.008,
        });
        world.insert_resource(PerformanceLogConfig {
            console: true,
            csv: false,
        });
        refresh_performance(&mut world);

        let text = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_perf_fps".into()))
            .and_then(|entity| {
                world
                    .get_component_store::<UiText>()
                    .and_then(|store| store.get_component(entity.index).cloned())
            })
            .unwrap();
        assert!(text.value.contains("120.0"), "{}", text.value);
        assert!(text.value.contains("8.00 ms"), "{}", text.value);

        let toggle = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_perf_console".into()))
            .and_then(|entity| {
                world
                    .get_component_store::<UiWidget>()
                    .and_then(|store| store.get_component(entity.index).cloned())
            })
            .unwrap();
        assert!(matches!(toggle.spec, UiWidgetSpec::Toggle { on: true }));
    }
}
