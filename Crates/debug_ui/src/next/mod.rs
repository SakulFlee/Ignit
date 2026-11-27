//! New-UI debug overlay, ported panel by panel from the iced implementation.
//!
//! This module is opt-in while the port is in progress: add
//! [`DebugUiNextModule`] after `UiRenderModule`, then press its toggle key.
//! The existing iced-backed [`crate::DebugUiModule`] stays available until the
//! port is complete and iced is removed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ignit_app::{Module, UiSystem};
use ignit_ecs::{Entity, IntoSystem, System, World};
use ignit_ecs_bridge::{
    DebugSelectedEntity, FpsStats, FrameTimings, PerformanceLogConfig, TimingsHistoryConfig,
};
use ignit_ui::{
    UiAction, UiBuilder, UiDirty, UiDocumentNode, UiEvent, UiGraphSeries, UiInteraction, UiLayout,
    UiStyle, UiText, UiView, UiWidget, UiWidgetSpec,
};
use ignit_ui_render::UiTextResource;
use log::{debug, warn};
use winit::keyboard::KeyCode;

use crate::state::{DebugMessage, DebugUiState, GraphMetric, GraphSeries, PanelId, format_seconds};

mod animation;
mod console;
mod ecs;
mod rendering;

const REFRESH: Duration = Duration::from_millis(33);
const PANEL_ID: &str = "debug_next_perf_window";
const ROOT_ID: &str = "debug_next_root";
/// At most this many graphs are shown at once.
const GRAPH_SLOTS: usize = 3;
/// Bars drawn per series. Longer windows are aggregated into this many
/// buckets so the bars keep their width instead of squeezing more samples in.
const MAX_GRAPH_SAMPLES: usize = 120;
/// Rolling window used by the average/min/max metrics, in seconds.
const ROLLING_WINDOW_SECS: f64 = 1.0;
/// Values above this are treated as timestamp-readback garbage.
const MAX_GRAPH_VALUE: f32 = 10_000.0;

const LAST_COLOR: [f32; 4] = [0.50, 0.90, 0.60, 1.0];
const AVG_COLOR: [f32; 4] = [0.40, 0.80, 1.00, 1.0];
const MIN_COLOR: [f32; 4] = [0.45, 0.60, 1.00, 1.0];
const MAX_COLOR: [f32; 4] = [1.00, 0.80, 0.40, 1.0];

const METRICS: [GraphMetric; 5] = [
    GraphMetric::Last,
    GraphMetric::Avg,
    GraphMetric::Min,
    GraphMetric::Max,
    GraphMetric::All,
];

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
            if visible {
                // The toolbar can reopen them; don't cover the screen with
                // every panel on the first show.
                crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Console), world);
                crate::handle_debug_message_world(
                    DebugMessage::HidePanel(PanelId::Rendering),
                    world,
                );
                crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Ecs), world);
                crate::handle_debug_message_world(
                    DebugMessage::HidePanel(PanelId::Animation),
                    world,
                );
            }
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
        rendering::refresh(world);
        animation::refresh(world);
        ecs::refresh(world);
    }
}

fn handle_events(world: &mut World, panel: Entity) {
    let events: Vec<UiEvent> = world
        .get_resource::<ignit_ecs::Events<UiEvent>>()
        .map(|events| events.read().cloned().collect())
        .unwrap_or_default();
    for event in events {
        if animation::handle_event(world, &event) || rendering::handle_event(world, &event) {
            continue;
        }
        if ecs::handle_event(world, &event) {
            continue;
        }
        match event {
            UiEvent::ValueChanged { id, value, .. } if id.as_str() == "debug_perf_window" => {
                if let Ok(seconds) = value.parse::<f32>()
                    && seconds.is_finite()
                {
                    crate::handle_debug_message_world(DebugMessage::SetGraphWindow(seconds), world);
                }
            }
            UiEvent::Action { action, .. } if action.name == "debug_perf_series_fps" => {
                crate::handle_debug_message_world(
                    DebugMessage::ToggleGraphSeries(GraphSeries::Fps),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name == "debug_perf_series_cdt" => {
                crate::handle_debug_message_world(
                    DebugMessage::ToggleGraphSeries(GraphSeries::Cdt),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name.starts_with("debug_perf_series_") => {
                if let Some(index) = stage_series_action(&action.name)
                    && index < ignit_ecs_bridge::TIMING_STAGES.len()
                {
                    crate::handle_debug_message_world(
                        DebugMessage::ToggleGraphSeries(GraphSeries::Stage(index)),
                        world,
                    );
                }
            }
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
            UiEvent::Action { action, .. } if action.name == "debug_toggle_perf" => {
                crate::handle_debug_message_world(
                    DebugMessage::TogglePanel(PanelId::Performance),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name == "debug_toggle_console" => {
                crate::handle_debug_message_world(
                    DebugMessage::TogglePanel(PanelId::Console),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name == "debug_toggle_ecs" => {
                crate::handle_debug_message_world(DebugMessage::TogglePanel(PanelId::Ecs), world);
            }
            UiEvent::Action { action, .. } if action.name == "debug_toggle_rendering" => {
                crate::handle_debug_message_world(
                    DebugMessage::TogglePanel(PanelId::Rendering),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name == "debug_toggle_animation" => {
                crate::handle_debug_message_world(
                    DebugMessage::TogglePanel(PanelId::Animation),
                    world,
                );
            }
            UiEvent::Action { action, .. } if action.name == "debug_hide_all" => {
                crate::handle_debug_message_world(DebugMessage::HideDebugUi, world);
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
        .child(
            UiDocumentNode::new("debug_perf_fps")
                .text("FPS: --")
                .action(UiAction::new("debug_perf_series_fps"))
                .focusable()
                .style(UiStyle::default().padding_text(ignit_ui::UiTextStyle {
                    size: 22.0,
                    weight: 600,
                    ..Default::default()
                })),
        )
        .child(
            UiDocumentNode::new("debug_perf_frame")
                .text("Frame: -- ms")
                .action(UiAction::new("debug_perf_series_cdt"))
                .focusable(),
        )
        .child(
            UiDocumentNode::new("debug_perf_window_row")
                .layout(
                    UiLayout::row()
                        .width_full()
                        .height(24.0)
                        .gap(6.0)
                        .shrink(0.0),
                )
                .child(
                    UiDocumentNode::new("debug_perf_window_label")
                        .layout(UiLayout::default().width(92.0))
                        .text("window 10s")
                        .style(small_style(true)),
                )
                .child(
                    UiDocumentNode::new("debug_perf_window")
                        .layout(UiLayout {
                            flex_grow: 1.0,
                            height: ignit_ui::UiDimension::Length(22.0),
                            ..Default::default()
                        })
                        .widget(UiWidgetSpec::Slider {
                            value: crate::state::DEFAULT_GRAPH_WINDOW_SECS,
                            min: crate::state::MIN_GRAPH_WINDOW_SECS,
                            max: crate::state::MAX_GRAPH_WINDOW_SECS,
                        })
                        .focusable(),
                ),
        )
        .child(
            UiDocumentNode::new("debug_perf_metric_label")
                .text("metric")
                .style(small_style(true)),
        )
        .child(
            UiDocumentNode::new("debug_perf_metric")
                .layout(UiLayout::default().width_full().height(38.0).shrink(0.0))
                .widget(UiWidgetSpec::Dropdown {
                    options: METRICS
                        .iter()
                        .map(|metric| metric_label(*metric).to_string())
                        .collect(),
                    selected: 0,
                    open: false,
                })
                .focusable()
                .style(small_style(false)),
        )
        .child(
            UiDocumentNode::new("debug_perf_graph_empty")
                .text("no graphs pinned — click a row")
                .style(small_style(true)),
        );

    for slot in 0..GRAPH_SLOTS {
        content = content.child(
            UiDocumentNode::new(format!("debug_perf_graph_slot_{slot}"))
                .layout(
                    UiLayout::column()
                        .width_full()
                        .height(108.0)
                        .gap(2.0)
                        .shrink(0.0),
                )
                .style(UiStyle::default().visible(false))
                .child(
                    UiDocumentNode::new(format!("debug_perf_graph_title_{slot}"))
                        .text("--")
                        .style(small_style(false)),
                )
                .child(
                    UiDocumentNode::new(format!("debug_perf_graph_{slot}"))
                        .layout(UiLayout::default().width_full().height(92.0).shrink(0.0))
                        .widget(UiWidgetSpec::Graph {
                            series: Vec::new(),
                            max: 1.0,
                        }),
                ),
        );
    }

    content = content.child(
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
                )
                .action(UiAction::new(format!("debug_perf_series_{index}")))
                .focusable(),
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
        .child(toolbar())
        .child(panel)
        .child(console::build())
        .child(rendering::build())
        .child(animation::build())
        .child(ecs::build());
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

/// Parses `debug_perf_series_<index>` into the stage index it toggles.
fn stage_series_action(name: &str) -> Option<usize> {
    name.strip_prefix("debug_perf_series_")
        .and_then(|value| value.parse().ok())
}

fn metric_label(metric: GraphMetric) -> &'static str {
    match metric {
        GraphMetric::Last => "last",
        GraphMetric::Avg => "avg",
        GraphMetric::Min => "min",
        GraphMetric::Max => "max",
        GraphMetric::All => "all",
    }
}

/// One ready-to-paint graph.
struct GraphPlot {
    title: String,
    series: Vec<UiGraphSeries>,
    max: f32,
}

/// Builds the plotted series for one graph.
///
/// The graph window comes from [`DebugUiState::graph_window_secs`]. Metrics
/// use a fixed one-second rolling window per retained frame; `All` draws the
/// four statistics as separate series.
fn graph_plot(
    timings: &FrameTimings,
    series: GraphSeries,
    metric: GraphMetric,
    window_secs: f64,
) -> GraphPlot {
    let start = timings.window_start(window_secs);
    let pairs = graph_pairs(timings, series, start);
    let times = pairs.iter().map(|(time, _)| *time).collect::<Vec<_>>();
    let values = pairs.iter().map(|(_, value)| *value).collect::<Vec<_>>();
    let frames = values.len();

    let plotted = match metric {
        GraphMetric::Last => vec![UiGraphSeries::new(
            downsample(&values, Downsample::Last),
            LAST_COLOR,
        )],
        GraphMetric::Avg => {
            let (average, _, _) = rolling_stats(&times, &values, ROLLING_WINDOW_SECS);
            vec![UiGraphSeries::new(
                downsample(&average, Downsample::Mean),
                AVG_COLOR,
            )]
        }
        GraphMetric::Min => {
            let (_, minimum, _) = rolling_stats(&times, &values, ROLLING_WINDOW_SECS);
            vec![UiGraphSeries::new(
                downsample(&minimum, Downsample::Min),
                MIN_COLOR,
            )]
        }
        GraphMetric::Max => {
            let (_, _, maximum) = rolling_stats(&times, &values, ROLLING_WINDOW_SECS);
            vec![UiGraphSeries::new(
                downsample(&maximum, Downsample::Max),
                MAX_COLOR,
            )]
        }
        GraphMetric::All => {
            let (average, minimum, maximum) = rolling_stats(&times, &values, ROLLING_WINDOW_SECS);
            vec![
                UiGraphSeries::new(downsample(&values, Downsample::Last), LAST_COLOR),
                UiGraphSeries::new(downsample(&average, Downsample::Mean), AVG_COLOR),
                UiGraphSeries::new(downsample(&minimum, Downsample::Min), MIN_COLOR),
                UiGraphSeries::new(downsample(&maximum, Downsample::Max), MAX_COLOR),
            ]
        }
    };

    let peak = plotted
        .iter()
        .flat_map(|series| series.values.iter().copied())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .fold(0.0f32, f32::max);

    GraphPlot {
        title: graph_title(series, frames, window_secs),
        series: plotted,
        max: (peak * 1.1).max(1.0),
    }
}

fn graph_title(series: GraphSeries, frames: usize, window_secs: f64) -> String {
    let name = series_name(series);
    let unit = if matches!(series, GraphSeries::Fps) {
        "fps"
    } else {
        "ms"
    };
    format!(
        "{name} · {unit} · {:.1}s · {} frames",
        window_secs.max(0.0),
        frames
    )
}

fn series_name(series: GraphSeries) -> &'static str {
    match series {
        GraphSeries::Fps => "FPS",
        GraphSeries::Cdt => "Frame time",
        GraphSeries::Stage(index) => ignit_ecs_bridge::TIMING_STAGES
            .get(index)
            .copied()
            .unwrap_or("stage"),
    }
}

fn graph_pairs(timings: &FrameTimings, series: GraphSeries, start: usize) -> Vec<(f64, f32)> {
    let times = timings.history_times();

    match series {
        GraphSeries::Stage(index) => timings
            .history()
            .iter()
            .zip(times.iter())
            .skip(start)
            .filter_map(|(frame, time)| {
                let value = *frame.get(index)? as f32;
                sane_graph_value(value).map(|value| (*time, value))
            })
            .collect(),
        GraphSeries::Cdt => timings
            .cdt_history()
            .iter()
            .zip(times.iter())
            .skip(start)
            .filter_map(|(value, time)| sane_graph_value(*value).map(|value| (*time, value)))
            .collect(),
        GraphSeries::Fps => timings
            .cdt_history()
            .iter()
            .zip(times.iter())
            .skip(start)
            .filter_map(|(&cdt, time)| {
                if cdt > 0.0 && cdt.is_finite() {
                    sane_graph_value(1000.0 / cdt).map(|value| (*time, value))
                } else {
                    None
                }
            })
            .collect(),
    }
}

fn sane_graph_value(value: f32) -> Option<f32> {
    (value.is_finite() && (0.0..=MAX_GRAPH_VALUE).contains(&value)).then_some(value)
}

#[derive(Clone, Copy)]
enum Downsample {
    Last,
    Mean,
    Min,
    Max,
}

/// Aggregates a whole history window into at most [`MAX_GRAPH_SAMPLES`] bars.
///
/// The window is never truncated: every selected frame contributes to a
/// bucket, so a longer window shows more history at the same bar width.
fn downsample(values: &[f32], mode: Downsample) -> Vec<f32> {
    if values.len() <= MAX_GRAPH_SAMPLES {
        return values.to_vec();
    }

    let mut buckets = Vec::with_capacity(MAX_GRAPH_SAMPLES);
    for bucket in 0..MAX_GRAPH_SAMPLES {
        let start = bucket * values.len() / MAX_GRAPH_SAMPLES;
        let end = (bucket + 1) * values.len() / MAX_GRAPH_SAMPLES;
        let slice = &values[start..end];
        let value = match mode {
            Downsample::Last => *slice.last().unwrap_or(&0.0),
            Downsample::Mean => slice.iter().copied().sum::<f32>() / slice.len().max(1) as f32,
            Downsample::Min => slice.iter().copied().fold(f32::INFINITY, f32::min),
            Downsample::Max => slice.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        };
        buckets.push(value);
    }

    buckets
}

/// Rolling mean/min/max over a time window, one sample per input frame.
fn rolling_stats(
    times: &[f64],
    values: &[f32],
    window_secs: f64,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let len = times.len().min(values.len());
    let mut average = Vec::with_capacity(len);
    let mut minimum = Vec::with_capacity(len);
    let mut maximum = Vec::with_capacity(len);

    if len == 0 {
        return (average, minimum, maximum);
    }

    let window = window_secs.max(0.0);
    let mut start = 0usize;
    let mut sum = 0.0f64;
    let mut min_queue = VecDeque::new();
    let mut max_queue = VecDeque::new();

    for index in 0..len {
        sum += values[index] as f64;

        while start < index && times[index] - times[start] > window {
            sum -= values[start] as f64;
            start += 1;
        }

        while min_queue.front().is_some_and(|front| *front < start) {
            min_queue.pop_front();
        }
        while max_queue.front().is_some_and(|front| *front < start) {
            max_queue.pop_front();
        }

        while min_queue
            .back()
            .is_some_and(|back| values[*back] >= values[index])
        {
            min_queue.pop_back();
        }
        min_queue.push_back(index);

        while max_queue
            .back()
            .is_some_and(|back| values[*back] <= values[index])
        {
            max_queue.pop_back();
        }
        max_queue.push_back(index);

        let count = (index - start + 1) as f64;
        average.push((sum / count) as f32);
        minimum.push(values[min_queue.front().copied().unwrap_or(index)]);
        maximum.push(values[max_queue.front().copied().unwrap_or(index)]);
    }

    (average, minimum, maximum)
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

fn toolbar() -> UiDocumentNode {
    let mut row = UiDocumentNode::new("debug_next_toolbar")
        .layout(
            UiLayout::row()
                .width_full()
                .height(28.0)
                .padding(4.0)
                .gap(4.0)
                .align_items(ignit_ui::UiAlign::Center),
        )
        .style(
            UiStyle::default()
                .background(ignit_ui::UiColor::Rgba([0.07, 0.08, 0.12, 0.96]))
                .padding_text(ignit_ui::UiTextStyle {
                    size: 11.0,
                    wrap: ignit_ui::UiTextWrap::NoWrap,
                    ..Default::default()
                }),
        )
        .child(
            UiDocumentNode::new("debug_toolbar_label")
                .layout(UiLayout::default().width(84.0))
                .text("Debug UI"),
        );
    for (id, label, action, width) in [
        (
            "debug_toolbar_perf",
            "Performance",
            "debug_toggle_perf",
            116.0,
        ),
        ("debug_toolbar_ecs", "ECS", "debug_toggle_ecs", 56.0),
        (
            "debug_toolbar_console",
            "Console",
            "debug_toggle_console",
            92.0,
        ),
        (
            "debug_toolbar_rendering",
            "Rendering",
            "debug_toggle_rendering",
            100.0,
        ),
        (
            "debug_toolbar_animation",
            "Animation",
            "debug_toggle_animation",
            100.0,
        ),
    ] {
        row = row.child(
            UiDocumentNode::new(id)
                .layout(
                    UiLayout::row()
                        .size(width, 22.0)
                        .justify(ignit_ui::UiJustify::Center)
                        .align_items(ignit_ui::UiAlign::Center),
                )
                .widget(UiWidgetSpec::Button {
                    label: label.into(),
                })
                .action(UiAction::new(action))
                .focusable()
                .style(UiStyle::default().class("panel"))
                .child(UiDocumentNode::new(format!("{id}_label")).text(label)),
        );
    }
    row = row.child(
        UiDocumentNode::new("debug_toolbar_spacer").layout(UiLayout {
            flex_grow: 1.0,
            height: ignit_ui::UiDimension::Length(1.0),
            ..Default::default()
        }),
    );
    row.child(
        UiDocumentNode::new("debug_toolbar_hide")
            .layout(
                UiLayout::row()
                    .size(64.0, 22.0)
                    .justify(ignit_ui::UiJustify::Center)
                    .align_items(ignit_ui::UiAlign::Center),
            )
            .widget(UiWidgetSpec::Button {
                label: "Hide".into(),
            })
            .action(UiAction::new("debug_hide_all"))
            .focusable()
            .style(UiStyle::default().class("panel"))
            .child(UiDocumentNode::new("debug_toolbar_hide_label").text("Hide")),
    )
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

    let (graph_series, metric, window_secs) = world
        .get_resource::<DebugUiState>()
        .map(|state| {
            (
                state.graph_series.clone(),
                state.graph_metric,
                state.graph_window_secs as f64,
            )
        })
        .unwrap_or_default();

    let (plots, rows) = world.get_resource::<FrameTimings>().map_or_else(
        || (Vec::new(), Vec::new()),
        |timings| {
            let plots = graph_series
                .iter()
                .copied()
                .filter(|series| match series {
                    GraphSeries::Stage(index) => *index < ignit_ecs_bridge::TIMING_STAGES.len(),
                    GraphSeries::Fps | GraphSeries::Cdt => true,
                })
                .take(GRAPH_SLOTS)
                .map(|series| graph_plot(&timings, series, metric, window_secs))
                .collect::<Vec<_>>();

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
                    let pinned = graph_series.contains(&GraphSeries::Stage(index));
                    (sample.name.to_string(), value, color, pinned)
                })
                .collect();
            (plots, rows)
        },
    );

    set_slider(world, "debug_perf_window", window_secs as f32);
    set_text(
        world,
        "debug_perf_window_label",
        format!("window {}s", format_seconds(window_secs as f32)),
    );
    let metric_index = METRICS
        .iter()
        .position(|value| *value == metric)
        .unwrap_or(0);
    poll_metric_dropdown(world, metric_index);

    for slot in 0..GRAPH_SLOTS {
        let slot_id = format!("debug_perf_graph_slot_{slot}");
        match plots.get(slot) {
            Some(plot) => {
                set_text(
                    world,
                    &format!("debug_perf_graph_title_{slot}"),
                    plot.title.clone(),
                );
                set_graph(
                    world,
                    &format!("debug_perf_graph_{slot}"),
                    plot.series.clone(),
                    plot.max,
                );
                set_visible_by_id(world, &slot_id, true);
            }
            None => set_visible_by_id(world, &slot_id, false),
        }
    }
    set_visible_by_id(world, "debug_perf_graph_empty", plots.is_empty());

    set_row_highlight(
        world,
        "debug_perf_fps",
        graph_series.contains(&GraphSeries::Fps),
    );
    set_row_highlight(
        world,
        "debug_perf_frame",
        graph_series.contains(&GraphSeries::Cdt),
    );
    for (index, (name, value, color, pinned)) in rows.into_iter().enumerate() {
        let name = if pinned { format!("● {name}") } else { name };
        set_text(world, &format!("debug_perf_stage_name_{index}"), name);
        set_text(world, &format!("debug_perf_stage_{index}"), value);
        set_text_color(world, &format!("debug_perf_stage_{index}"), color);
        set_row_highlight(world, &format!("debug_perf_stage_row_{index}"), pinned);
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

fn set_row_highlight(world: &mut World, id: &str, selected: bool) {
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
    let background = selected.then(|| ignit_ui::UiColor::rgba([0.28, 0.42, 0.68, 0.35]));
    if style.background == background {
        return;
    }
    style.background = background;
    let _ = world.attach_component(&entity, style);
    mark_dirty(world, entity, UiDirty::PAINT);
}

fn set_graph(world: &mut World, id: &str, series: Vec<UiGraphSeries>, max: f32) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Graph {
            series: current,
            max: current_max,
        } = &mut widget.spec
        && (*current != series || *current_max != max)
    {
        *current = series;
        *current_max = max;
        changed = true;
    }
    if changed {
        mark_dirty(world, entity, UiDirty::PAINT);
    }
}

fn set_slider(world: &mut World, id: &str, value: f32) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Slider { value: current, .. } = &mut widget.spec
        && *current != value
    {
        *current = value;
        changed = true;
    }
    if changed {
        mark_dirty(world, entity, UiDirty::PAINT);
    }
}

/// The dropdown owns its selection; dispatch when it no longer matches the
/// state, mirroring the other polled dropdowns in the new UI.
fn poll_metric_dropdown(world: &mut World, expected: usize) {
    let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&"debug_perf_metric".into()))
    else {
        return;
    };
    let selected = world
        .get_component_store::<UiWidget>()
        .and_then(|store| store.get_component(entity.index).cloned())
        .and_then(|widget| match widget.spec {
            UiWidgetSpec::Dropdown { selected, .. } => Some(selected),
            _ => None,
        });
    if let Some(selected) = selected
        && selected != expected
        && let Some(metric) = METRICS.get(selected).copied()
    {
        crate::handle_debug_message_world(DebugMessage::SetGraphMetric(metric), world);
    }
}

fn set_visible_by_id(world: &mut World, id: &str, visible: bool) {
    if let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
    {
        set_visible(world, entity, visible);
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
        crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Ecs), &mut world);
        crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Animation), &mut world);
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
    fn floating_windows_clip_all_descendants_after_resize() {
        let mut world = World::new();
        let (root, _) = build_document(&mut world).expect("document");
        set_visible(&mut world, root, true);

        let windows = [
            ("debug_next_perf_window", "debug_next_perf_scroll"),
            ("debug_next_console_window", "debug_console_body"),
            ("debug_render_window", "debug_render_scroll"),
            ("debug_anim_window", "debug_anim_body"),
            ("debug_ecs_window", "debug_ecs_body"),
        ];

        for (window_id, _) in windows {
            let window = {
                let forest = world.get_resource::<ignit_ui::UiForest>().unwrap();
                forest.find(&window_id.into()).unwrap()
            };
            let mut layout = world
                .get_component_store::<ignit_ui::UiLayout>()
                .unwrap()
                .get_component(window.index)
                .cloned()
                .unwrap();
            layout.width = ignit_ui::UiDimension::Length(240.0);
            layout.height = ignit_ui::UiDimension::Length(160.0);
            world.attach_component(&window, layout).unwrap();
            mark_dirty(&mut world, window, UiDirty::LAYOUT | UiDirty::PAINT);
        }

        let mut text = ignit_ui::UiFallbackTextMeasure;
        ignit_ui::update(&mut world, &mut text);
        ignit_ui::update(&mut world, &mut text);

        let entities: Vec<_> = windows
            .iter()
            .map(|(window_id, descendant_id)| {
                let forest = world.get_resource::<ignit_ui::UiForest>().unwrap();
                (
                    *window_id,
                    *descendant_id,
                    forest.find(&(*window_id).into()).unwrap(),
                    forest.find(&(*descendant_id).into()).unwrap(),
                )
            })
            .collect();

        let nodes = world.get_component_store::<ignit_ui::UiNode>().unwrap();
        let computed = world.get_component_store::<ignit_ui::UiComputed>().unwrap();
        for (window_id, descendant_id, window, descendant) in entities {
            let window_computed = computed.get_component(window.index).unwrap();
            let window_rect = window_computed.rect;
            assert_eq!(window_rect.width, 240.0, "{window_id} width");
            assert_eq!(window_rect.height, 160.0, "{window_id} height");

            let descendant_computed = computed.get_component(descendant.index).unwrap();
            let clip = descendant_computed
                .clip
                .unwrap_or_else(|| panic!("{descendant_id} has no clip"));
            assert!(
                clip.is_empty() || window_rect.intersect(clip) == Some(clip),
                "{descendant_id} clip {clip:?} escapes {window_id} {window_rect:?}"
            );

            let mut clipped = 0;
            for entity in world.entities() {
                let mut cursor = entity;
                let mut inside = false;
                while let Some(parent) = nodes
                    .get_component(cursor.index)
                    .and_then(|node| node.parent)
                {
                    if parent == window {
                        inside = true;
                        break;
                    }
                    cursor = parent;
                }
                if !inside {
                    continue;
                }
                let Some(entity_computed) = computed.get_component(entity.index) else {
                    continue;
                };
                let Some(clip) = entity_computed.clip else {
                    continue;
                };
                clipped += 1;
                let node = nodes.get_component(entity.index).unwrap();
                assert!(
                    clip.is_empty() || window_rect.intersect(clip) == Some(clip),
                    "{} clip {clip:?} escapes {window_id} {window_rect:?}",
                    node.id.as_str()
                );
            }
            assert!(clipped > 0, "{window_id} has no clipped descendants");
        }
    }

    #[test]
    fn graph_actions_toggle_series_and_the_metric_dropdown_updates_state() {
        let mut world = World::new();
        world.insert_resource(DebugUiState::new(KeyCode::F1));
        let (_, panel) = build_document(&mut world).expect("document");
        let stage = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_perf_stage_row_0".into()))
            .expect("stage row");
        let fps = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_perf_fps".into()))
            .expect("fps row");
        {
            let mut events = world
                .get_resource_mut::<ignit_ecs::Events<UiEvent>>()
                .expect("ui events");
            events.send(UiEvent::Action {
                entity: stage,
                id: "debug_perf_stage_row_0".into(),
                action: UiAction::new("debug_perf_series_0"),
            });
            events.send(UiEvent::Action {
                entity: fps,
                id: "debug_perf_fps".into(),
                action: UiAction::new("debug_perf_series_fps"),
            });
        }

        handle_events(&mut world, panel);

        {
            let state = world.get_resource::<DebugUiState>().expect("state");
            assert!(state.graph_series.contains(&GraphSeries::Stage(0)));
            assert!(state.graph_series.contains(&GraphSeries::Fps));
        }

        let metric = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&"debug_perf_metric".into()))
            .expect("metric dropdown");
        {
            let store = world
                .get_component_store_mut::<UiWidget>()
                .expect("widgets");
            let widget = store
                .get_mut_store()
                .get_component_mut(metric.index)
                .expect("metric widget");
            let UiWidgetSpec::Dropdown { selected, .. } = &mut widget.spec else {
                panic!("metric selector is a dropdown");
            };
            *selected = METRICS
                .iter()
                .position(|metric| *metric == GraphMetric::All)
                .expect("All metric");
        }

        refresh_performance(&mut world);

        assert_eq!(
            world
                .get_resource::<DebugUiState>()
                .expect("state")
                .graph_metric,
            GraphMetric::All
        );
    }

    #[test]
    fn clicking_a_stage_row_toggles_that_rows_series() {
        let mut world = World::new();
        world.insert_resource(DebugUiState::new(KeyCode::F1));
        let (root, panel) = build_document(&mut world).expect("document");
        set_visible(&mut world, root, true);
        let mut text = ignit_ui::UiFallbackTextMeasure;
        ignit_ui::update(&mut world, &mut text);

        let index = 3;
        let row = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&format!("debug_perf_stage_row_{index}").into()))
            .expect("stage row");
        let rect = world
            .get_component_store::<ignit_ui::UiComputed>()
            .and_then(|store| store.get_component(row.index).copied())
            .expect("row rect");
        let pointer = ignit_ui::UiPoint::new(rect.rect.x + 20.0, rect.rect.y + 8.0);
        world.insert_resource(ignit_ui::UiInputState {
            pointer,
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        });
        ignit_ui::update(&mut world, &mut text);
        let mut input = world
            .get_resource::<ignit_ui::UiInputState>()
            .expect("input")
            .clone();
        input.primary_down = false;
        input.primary_pressed = false;
        input.primary_released = true;
        world.insert_resource(input);
        ignit_ui::update(&mut world, &mut text);
        handle_events(&mut world, panel);

        let state = world.get_resource::<DebugUiState>().expect("state");
        assert!(state.graph_series.contains(&GraphSeries::Stage(index)));
        assert!(
            !state.graph_series.contains(&GraphSeries::Stage(
                ignit_ecs_bridge::TIMING_STAGES.len() - 1
            )),
            "the bottom row must not be toggled"
        );
    }

    #[test]
    fn graph_plot_samples_the_selected_window_instead_of_truncating_it() {
        let mut timings = FrameTimings::new();
        let config = TimingsHistoryConfig {
            enabled: true,
            window_secs: 150.0,
        };
        let mut durations = vec![0.0; ignit_ecs_bridge::TIMING_STAGES.len()];
        for index in 0..300 {
            durations[0] = index as f64;
            timings.record_at(&durations, index as f64 * 0.5, 16.0, config);
        }

        let wide = graph_plot(&timings, GraphSeries::Stage(0), GraphMetric::Last, 150.0);
        assert_eq!(wide.series[0].values.len(), MAX_GRAPH_SAMPLES);
        assert!(
            (wide.series[0].values[0] - 1.0).abs() < 1.0e-4,
            "the first bucket keeps the last raw frame it contains"
        );

        let narrow = graph_plot(&timings, GraphSeries::Stage(0), GraphMetric::Last, 10.0);
        assert!(
            narrow.series[0].values.len() < MAX_GRAPH_SAMPLES,
            "a short window keeps its own sample count"
        );
        assert!(
            (narrow.series[0].values[0] - 279.0).abs() < 1.0e-4,
            "the short window starts at the window cutoff"
        );
    }

    #[test]
    fn graph_plot_builds_the_all_band_from_rolling_stats() {
        let mut timings = FrameTimings::new();
        let config = TimingsHistoryConfig {
            enabled: true,
            window_secs: 10.0,
        };
        let mut durations = vec![0.0; ignit_ecs_bridge::TIMING_STAGES.len()];
        for (index, value) in [1.0, 3.0, 2.0, 4.0].into_iter().enumerate() {
            durations[0] = value;
            timings.record_at(&durations, index as f64 * 0.5, 16.0, config);
        }

        let plot = graph_plot(&timings, GraphSeries::Stage(0), GraphMetric::All, 10.0);

        assert_eq!(plot.series.len(), 4, "last, avg, min, max");
        assert_eq!(plot.series[0].values, vec![1.0, 3.0, 2.0, 4.0]);
        assert_eq!(plot.series[1].values, vec![1.0, 2.0, 2.0, 3.0]);
        assert_eq!(plot.series[2].values, vec![1.0, 1.0, 1.0, 2.0]);
        assert_eq!(plot.series[3].values, vec![1.0, 3.0, 3.0, 4.0]);
        assert!((plot.max - 4.4).abs() < 1.0e-4, "peak plus ten percent");
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
