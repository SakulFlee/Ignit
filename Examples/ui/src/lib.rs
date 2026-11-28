use std::sync::{Arc, Mutex};

use ignit::app::{App, AppSettings, Module, UiSystem};
use ignit::cgmath::{Point3, Rad};
use ignit::ecs::{System, World};
use ignit::ecs_bridge::{
    ActiveCamera, CameraDescriptor, EnvironmentDescriptorResource, Position, Rotation,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use ignit::logging;
use ignit::logging::{debug, error, info, trace, warn};
use ignit::resources::WorldEnvironmentDescriptor;
use ignit::ui::{
    UiAction, UiBuilder, UiColor, UiDimension, UiDocument, UiDocumentNode, UiEdges, UiEvent,
    UiGraphSeries, UiGraphStyle, UiLayout, UiLengthPercentage, UiNotification,
    UiNotificationSeverity, UiNotifications, UiStyle, UiTextStyle, UiTextWrap, UiTheme, UiThemes,
    UiView, UiWidget, UiWidgetSpec,
};
use ignit::ui_render::UiRenderModule;
use ignit::ui_text::TextEngine;

pub const NAME: &str = env!("CARGO_PKG_NAME");

#[ignit::main]
pub fn entrypoint(
    event_loop_result: Result<
        ignit::winit::event_loop::EventLoop<()>,
        ignit::winit::error::EventLoopError,
    >,
) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    logging::init();

    let event_loop = event_loop_result.expect("Event Loop failure");
    let text = Arc::new(Mutex::new(TextEngine::new()));
    let app_settings = AppSettings {
        fullscreen: false,
        name: NAME.to_string(),
        back_presses_to_exit: 3,
        ..AppSettings::default()
    };

    match App::new()
        .add_module(UiRenderModule::new(text))
        .add_module(UiDemoModule)
        .add_module(ignit::debug_ui::DebugUiNextModule::new())
        .liftoff(event_loop, app_settings)
    {
        Ok(()) => info!("Cleanly exited!"),
        Err(error) => error!("Runtime failure: {error:?}"),
    }
}

struct UiDemoModule;

impl Module for UiDemoModule {
    fn setup(
        &self,
        ecs: &mut World,
        _device: &ignit::wgpu::Device,
        _queue: &ignit::wgpu::Queue,
    ) -> Vec<Box<dyn System>> {
        ignit::ui::init(ecs);
        install_themes(ecs);
        error!("UI demo log sample: error");
        warn!("UI demo log sample: warning");
        info!("UI demo log sample: info");
        debug!("UI demo log sample: debug");
        trace!("UI demo log sample: trace");
        if let Some(mut notifications) = ecs.get_resource_mut::<UiNotifications>() {
            notifications.max_visible = 6;
        }

        disable_world_environment(ecs);

        let camera = ecs.spawn_entity();
        ecs.attach_component(
            &camera,
            CameraDescriptor {
                label: "UI demo camera".into(),
                aspect: 16.0 / 9.0,
                fovy: Rad(std::f32::consts::FRAC_PI_4),
                projection_mode: Default::default(),
                ortho_height: 10.0,
                near: 0.1,
                far: 100.0,
                global_gamma: 2.2,
            },
        )
        .unwrap();
        ecs.attach_component(&camera, Position(Point3::new(0.0, 1.0, 5.0)))
            .unwrap();
        ecs.attach_component(&camera, Rotation::identity()).unwrap();
        ecs.insert_resource(ActiveCamera(camera));

        build_showcase(ecs);
        Vec::new()
    }

    fn ui_systems(&self, _ecs: &mut World) -> Vec<Box<dyn UiSystem>> {
        vec![Box::new(DemoSystem::new())]
    }
}

/// The UI showcase does not need a 3D backdrop. Explicitly opt out of the
/// default generated skybox/IBL environment.
fn disable_world_environment(world: &mut World) {
    world.insert_resource(EnvironmentDescriptorResource(Some(
        WorldEnvironmentDescriptor::None,
    )));
}

fn install_themes(world: &mut World) {
    let dark = UiTheme::dark();
    let light = UiTheme::light();
    let mut themes = UiThemes::with_active(dark.clone());
    themes.named.insert("dark".into(), dark);
    themes.named.insert("light".into(), light);
    world.insert_resource(themes);
}

fn build_showcase(world: &mut World) {
    let content = UiDocumentNode::new("demo_content")
        .layout(
            UiLayout::column()
                .width_full()
                .height_full()
                .padding(24.0)
                .gap(12.0)
                .shrink(0.0),
        )
        .scroll()
        .child(UiDocumentNode::new("title").text("Ignit UI widgets").style(
            UiStyle::default().padding_text(UiTextStyle {
                size: 28.0,
                weight: 600,
                ..Default::default()
            }),
        ))
        .child(
            UiDocumentNode::new("subtitle")
                .text("Code-authored and RON-authored screens · theme tokens · focus · editing")
                .style(UiStyle::default().class("muted")),
        )
        .child(
            UiDocumentNode::new("actions")
                .layout(UiLayout::row().height(44.0).gap(10.0))
                .child(
                    UiDocumentNode::new("theme_button")
                        .layout(
                            UiLayout::row()
                                .size(170.0, 38.0)
                                .justify(ignit::ui::UiJustify::Center)
                                .align_items(ignit::ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Switch theme".into(),
                        })
                        .action(UiAction::new("theme"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("theme_label").text("Switch theme")),
                )
                .child(
                    UiDocumentNode::new("modal_button")
                        .layout(
                            UiLayout::row()
                                .size(170.0, 38.0)
                                .justify(ignit::ui::UiJustify::Center)
                                .align_items(ignit::ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Toggle modal".into(),
                        })
                        .action(UiAction::new("modal"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("modal_label").text("Toggle modal")),
                )
                .child(
                    UiDocumentNode::new("notify_button")
                        .layout(
                            UiLayout::row()
                                .size(150.0, 38.0)
                                .justify(ignit::ui::UiJustify::Center)
                                .align_items(ignit::ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Notify".into(),
                        })
                        .action(UiAction::new("notify"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("notify_label").text("Notify")),
                )
                .child(
                    UiDocumentNode::new("notify_placement_button")
                        .layout(
                            UiLayout::row()
                                .size(170.0, 38.0)
                                .justify(ignit::ui::UiJustify::Center)
                                .align_items(ignit::ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Toast corner".into(),
                        })
                        .action(UiAction::new("toast_corner"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("toast_corner_label").text("Toast corner")),
                )
                .child(
                    UiDocumentNode::new("log_samples_button")
                        .layout(
                            UiLayout::row()
                                .size(140.0, 38.0)
                                .justify(ignit::ui::UiJustify::Center)
                                .align_items(ignit::ui::UiAlign::Center),
                        )
                        .widget(UiWidgetSpec::Button {
                            label: "Log samples".into(),
                        })
                        .action(UiAction::new("log_samples"))
                        .focusable()
                        .style(UiStyle::default().class("panel"))
                        .child(UiDocumentNode::new("log_samples_label").text("Log samples")),
                )
                .child(
                    UiDocumentNode::new("radio_demo")
                        .layout(
                            UiLayout::default()
                                .width(190.0)
                                .height(38.0)
                                .padding_edges(UiEdges {
                                    left: UiLengthPercentage::Length(8.0),
                                    right: UiLengthPercentage::Length(8.0),
                                    top: UiLengthPercentage::Zero,
                                    bottom: UiLengthPercentage::Zero,
                                })
                                .shrink(0.0),
                        )
                        .widget(UiWidgetSpec::RadioGroup {
                            options: vec!["Alpha".into(), "Beta".into()],
                            selected: 0,
                            horizontal: true,
                        })
                        .focusable()
                        .style(UiStyle::default().class("panel")),
                ),
        )
        .child(
            UiDocumentNode::new("controls")
                .layout(
                    UiLayout::row()
                        .height(120.0)
                        .gap(18.0)
                        .align_items(ignit::ui::UiAlign::Center),
                )
                .child(
                    UiDocumentNode::new("toggle")
                        .layout(UiLayout::default().size(56.0, 32.0))
                        .widget(UiWidgetSpec::Toggle { on: true })
                        .focusable()
                        .style(UiStyle::default().class("panel")),
                )
                .child(
                    UiDocumentNode::new("checkbox")
                        .layout(UiLayout::default().size(32.0, 32.0))
                        .widget(UiWidgetSpec::Checkbox { checked: true })
                        .focusable()
                        .style(UiStyle::default().class("panel")),
                )
                .child(
                    UiDocumentNode::new("slider")
                        .layout(UiLayout::default().width(220.0).height(32.0))
                        .widget(UiWidgetSpec::Slider {
                            value: 0.35,
                            min: 0.0,
                            max: 1.0,
                        })
                        .focusable(),
                )
                .child(
                    UiDocumentNode::new("progress")
                        .layout(UiLayout::default().width(180.0).height(24.0))
                        .widget(UiWidgetSpec::ProgressBar { value: 0.62 }),
                )
                .child(
                    UiDocumentNode::new("textbox")
                        .layout(UiLayout::default().width(220.0).height(38.0).padding_edges(
                            UiEdges {
                                left: UiLengthPercentage::Length(10.0),
                                right: UiLengthPercentage::Length(10.0),
                                top: UiLengthPercentage::Length(0.0),
                                bottom: UiLengthPercentage::Length(0.0),
                            },
                        ))
                        .widget(UiWidgetSpec::TextBox {
                            value: "Edit me".into(),
                            placeholder: "Type here".into(),
                            multiline: false,
                        })
                        .focusable()
                        .style(UiStyle::default().class("panel").padding_text(UiTextStyle {
                            size: 16.0,
                            wrap: UiTextWrap::NoWrap,
                            ..Default::default()
                        })),
                )
                .child(
                    UiDocumentNode::new("textarea")
                        .layout(
                            UiLayout::default()
                                .width(250.0)
                                .height(96.0)
                                .padding_edges(UiEdges {
                                    left: UiLengthPercentage::Length(10.0),
                                    right: UiLengthPercentage::Length(10.0),
                                    top: UiLengthPercentage::Length(6.0),
                                    bottom: UiLengthPercentage::Length(6.0),
                                }),
                        )
                        .widget(UiWidgetSpec::TextBox {
                            value: "Multi-line text area.\nArrow keys move between lines.\nHome/End stay on the line.".into(),
                            placeholder: "Write something".into(),
                            multiline: true,
                        })
                        .focusable()
                        .style(UiStyle::default().class("panel").padding_text(UiTextStyle {
                            size: 15.0,
                            ..Default::default()
                        })),
                )
                .child(
                    UiDocumentNode::new("dropdown")
                        .layout(UiLayout::default().width(180.0).height(38.0))
                        .widget(UiWidgetSpec::Dropdown {
                            options: vec!["Low".into(), "Medium".into(), "High".into()],
                            selected: 1,
                            open: false,
                        })
                        .focusable()
                        .style(UiStyle::default().class("panel")),
                ),
        )
        .child(
            UiDocumentNode::new("tabs")
                .layout(UiLayout::default().width(420.0).height(44.0).shrink(0.0))
                .widget(UiWidgetSpec::Tabs {
                    tabs: vec!["Overview".into(), "Rendering".into(), "Console".into()],
                    active: 0,
                    contents: vec![
                        "tab_overview".into(),
                        "tab_rendering".into(),
                        "tab_console".into(),
                    ],
                }),
        )
        .child(
            UiDocumentNode::new("tab_overview")
                .layout(UiLayout::column().height(44.0).padding(8.0).shrink(0.0))
                .style(UiStyle::default().class("panel"))
                .child(UiDocumentNode::new("tab_overview_text").text("Overview content")),
        )
        .child(
            UiDocumentNode::new("tab_rendering")
                .layout(UiLayout::column().height(44.0).padding(8.0).shrink(0.0))
                .style(UiStyle::default().class("panel").visible(false))
                .child(UiDocumentNode::new("tab_rendering_text").text("Rendering content")),
        )
        .child(
            UiDocumentNode::new("tab_console")
                .layout(UiLayout::column().height(44.0).padding(8.0).shrink(0.0))
                .style(UiStyle::default().class("panel").visible(false))
                .child(UiDocumentNode::new("tab_console_text").text("Console content")),
        )
        .child(
            UiDocumentNode::new("collapsible_header")
                .layout(
                    UiLayout::default()
                        .size(260.0, 36.0)
                        .padding_edges(UiEdges {
                            left: UiLengthPercentage::Length(10.0),
                            right: UiLengthPercentage::Length(10.0),
                            top: UiLengthPercentage::Length(0.0),
                            bottom: UiLengthPercentage::Length(0.0),
                        }),
                )
                .text("Collapsible section")
                .widget(UiWidgetSpec::Collapsible {
                    open: true,
                    content: Some("collapsible_content".into()),
                })
                .focusable()
                .style(UiStyle::default().class("panel")),
        )
        .child(
            UiDocumentNode::new("collapsible_content")
                .layout(UiLayout::column().height(80.0).padding(10.0))
                .style(UiStyle::default().class("panel"))
                .child(
                    UiDocumentNode::new("collapsible_text")
                        .text("Visible when the header is open."),
                ),
        )
        .child(
            UiDocumentNode::new("sparkline")
                .layout(UiLayout::default().width(360.0).height(80.0))
                .widget(UiWidgetSpec::Sparkline {
                    values: (0..48)
                        .map(|index| ((index as f32 * 0.4).sin() * 0.5 + 0.5) * 0.8 + 0.1)
                        .collect(),
                    max: 1.0,
                }),
        )
        .child(
            UiDocumentNode::new("scroll_panel")
                .layout(
                    UiLayout::column()
                        .width_full()
                        .height(220.0)
                        .padding(8.0)
                        .gap(6.0),
                )
                .style(UiStyle::default().class("panel"))
                .child(
                    UiDocumentNode::new("scroll_header")
                        .text("Scrollable list")
                        .style(UiStyle::default().padding_text(UiTextStyle {
                            size: 16.0,
                            weight: 600,
                            ..Default::default()
                        })),
                )
                .child(
                    UiDocumentNode::new("scroll")
                        .layout(UiLayout::default().width_full().height(160.0))
                        .scroll()
                        .child({
                            let mut rows = UiDocumentNode::new("scroll_rows")
                                .layout(UiLayout::column().gap(6.0).padding(8.0).shrink(0.0));
                            for index in 0..18 {
                                rows = rows.child(
                                    UiDocumentNode::new(format!("row_{index:02}"))
                                        .text(format!("Scrollable row {index:02}")),
                                );
                            }
                            rows
                        }),
                ),
        )
        .child(graph_showcase());

    let floating = UiDocumentNode::new("floating")
        .layout(
            UiLayout::column()
                .width(300.0)
                .height(140.0)
                .padding_edges(UiEdges {
                    left: UiLengthPercentage::Length(10.0),
                    right: UiLengthPercentage::Length(10.0),
                    top: UiLengthPercentage::Length(36.0),
                    bottom: UiLengthPercentage::Length(10.0),
                })
                .absolute()
                .inset(UiEdges {
                    left: UiDimension::Auto,
                    right: UiDimension::Length(24.0),
                    top: UiDimension::Length(24.0),
                    bottom: UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "Floating window".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel"))
        .child(
            UiDocumentNode::new("floating_body")
                .text("Floating panels overlay the scene. Docking is v1.8."),
        );

    let modal = UiDocumentNode::new("modal")
        .layout(
            UiLayout::column()
                .width(420.0)
                .height(220.0)
                .padding(18.0)
                .gap(10.0)
                .absolute()
                .inset(UiEdges {
                    left: UiDimension::Length(430.0),
                    right: UiDimension::Auto,
                    top: UiDimension::Length(250.0),
                    bottom: UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::Modal { open: false })
        .style(UiStyle::default().class("panel").visible(false))
        .child(UiDocumentNode::new("modal_title").text("Modal scope"))
        .child(
            UiDocumentNode::new("modal_body")
                .text("Focus is trapped in this modal while it is open."),
        );

    let mut root = UiDocumentNode::new("demo_root")
        .layout(UiLayout::column().width_full().height_full())
        .style(
            UiStyle::default()
                .background(UiColor::Token("bg".into()))
                .class("panel"),
        )
        .child(content)
        .child(floating)
        .child(modal);
    root.view = Some(UiView::new(1280.0, 720.0));
    UiBuilder::new().root(root).build(world).unwrap();

    let ron = include_str!("../Assets/Scenes/ui_demo.ron");
    if let Ok(document) = UiDocument::from_ron(ron) {
        let _ = ignit::ui::spawn_document(world, &document, Some("ui_demo"));
    }
}

const DEMO_GRAPH_SAMPLES: usize = 64;
const DEMO_GRAPH_COLORS: [[f32; 4]; 3] = [
    [0.20, 0.85, 1.00, 1.0],
    [0.72, 0.45, 1.00, 1.0],
    [1.00, 0.55, 0.25, 1.0],
];

fn demo_graph_value(phase: f32, series: usize, index: usize) -> f32 {
    let x = index as f32;
    let offset = series as f32 * 0.85;
    let value = 0.48
        + 0.24 * (x * 0.22 + phase + offset).sin()
        + 0.12 * (x * 0.61 - phase * 1.7 + offset * 0.4).cos()
        + 0.06 * (x * 1.4 + phase * 2.3).sin();
    value.clamp(0.02, 1.0)
}

fn demo_graph_series(phase: f32, phase_offset: f32) -> Vec<UiGraphSeries> {
    (0..DEMO_GRAPH_COLORS.len())
        .map(|series| {
            UiGraphSeries::new(
                (0..DEMO_GRAPH_SAMPLES)
                    .map(|index| demo_graph_value(phase + phase_offset, series, index))
                    .collect(),
                DEMO_GRAPH_COLORS[series],
            )
        })
        .collect()
}

fn graph_card(id: &str, title: &str, style: UiGraphStyle, phase_offset: f32) -> UiDocumentNode {
    UiDocumentNode::new(format!("{id}_card"))
        .layout(
            UiLayout::column()
                .width_percent(31.0)
                .height(166.0)
                .padding(8.0)
                .gap(6.0)
                .shrink(0.0),
        )
        .style(UiStyle::default().class("panel"))
        .child(
            UiDocumentNode::new(format!("{id}_title"))
                .text(title)
                .style(UiStyle::default().padding_text(UiTextStyle {
                    size: 13.0,
                    weight: 600,
                    ..Default::default()
                })),
        )
        .child(
            UiDocumentNode::new(id)
                .layout(UiLayout::default().width_full().height(120.0).shrink(0.0))
                .widget(UiWidgetSpec::Graph {
                    series: demo_graph_series(0.0, phase_offset),
                    max: 1.0,
                    style,
                    y_labels: vec!["0.0".into(), "0.5".into(), "1.0".into()],
                    x_labels: vec!["-10s".into(), "-5s".into(), "now".into()],
                }),
        )
}

fn graph_showcase() -> UiDocumentNode {
    UiDocumentNode::new("demo_graphs")
        .layout(
            UiLayout::row()
                .width(800.0)
                .height(180.0)
                .gap(12.0)
                .shrink(0.0),
        )
        .child(graph_card(
            "demo_graph_bars",
            "Bars · packed",
            UiGraphStyle::Bars,
            0.0,
        ))
        .child(graph_card(
            "demo_graph_line",
            "Line · stacked",
            UiGraphStyle::Line,
            0.7,
        ))
        .child(graph_card(
            "demo_graph_area",
            "Area · stacked",
            UiGraphStyle::Area,
            1.4,
        ))
}

struct DemoSystem {
    dark: bool,
    notices: u32,
    phase: f32,
}

impl DemoSystem {
    fn new() -> Self {
        Self {
            dark: true,
            notices: 0,
            phase: 0.0,
        }
    }

    fn apply_events(&mut self, world: &mut World) {
        let events: Vec<UiEvent> = world
            .get_resource::<ignit::ecs::Events<UiEvent>>()
            .map(|events| events.read().cloned().collect())
            .unwrap_or_default();
        for event in events {
            if let UiEvent::Click { id, .. } = &event {
                info!("UI demo click: {id}");
            }
            if let UiEvent::ValueChanged { id, value, .. } = &event {
                if id.as_str() == "slider"
                    && let Ok(slider_value) = value.parse::<f32>()
                {
                    let progress = world
                        .get_resource::<ignit::ui::UiForest>()
                        .and_then(|forest| forest.find(&"progress".into()));
                    if let Some(progress) = progress
                        && let Some(store) = world.get_component_store_mut::<UiWidget>()
                    {
                        let store = store.get_mut_store();
                        if let Some(widget) = store.get_component_mut(progress.index)
                            && let UiWidgetSpec::ProgressBar { value } = &mut widget.spec
                        {
                            *value = slider_value.clamp(0.0, 1.0);
                        }
                    }
                }
                if (id.as_str() == "toggle" || id.as_str() == "checkbox")
                    && let Ok(on) = value.parse::<bool>()
                {
                    let target = if id.as_str() == "toggle" {
                        "checkbox"
                    } else {
                        "toggle"
                    };
                    let entity = world
                        .get_resource::<ignit::ui::UiForest>()
                        .and_then(|forest| forest.find(&target.into()));
                    if let Some(entity) = entity
                        && let Some(store) = world.get_component_store_mut::<UiWidget>()
                    {
                        let store = store.get_mut_store();
                        if let Some(widget) = store.get_component_mut(entity.index) {
                            match &mut widget.spec {
                                UiWidgetSpec::Toggle { on: target_on } => {
                                    *target_on = on;
                                }
                                UiWidgetSpec::Checkbox { checked } => {
                                    *checked = on;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                continue;
            }
            let UiEvent::Action { action, id, .. } = event else {
                continue;
            };
            let action_name = action.name;
            info!("UI demo action: {action_name} ({id})");
            match action_name.as_str() {
                "theme" => {
                    self.dark = !self.dark;
                    if let Some(mut themes) = world.get_resource_mut::<UiThemes>() {
                        let target = if self.dark { "dark" } else { "light" };
                        if let Some(next) = themes.named.get(target).cloned() {
                            themes.active = next;
                        }
                    }
                }
                "log_samples" => {
                    error!("UI demo sample: error level");
                    warn!("UI demo sample: warning level");
                    info!("UI demo sample: info level");
                    debug!("UI demo sample: debug level");
                    trace!("UI demo sample: trace level");
                }
                "toast_corner" => {
                    if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
                        notifications.placement = notifications.placement.next();
                        info!("UI demo toast placement: {:?}", notifications.placement);
                    }
                }
                "notify" => {
                    self.notices = self.notices.wrapping_add(1);
                    let severity = match self.notices % 5 {
                        1 => UiNotificationSeverity::Info,
                        2 => UiNotificationSeverity::Success,
                        3 => UiNotificationSeverity::Warning,
                        4 => UiNotificationSeverity::Critical,
                        _ => UiNotificationSeverity::Debug,
                    };
                    if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
                        let notification = UiNotification::new(
                            severity,
                            format!("Demo notice #{}", self.notices),
                            "Toasts are drawn above every root and never capture input.",
                        );
                        notifications.post(notification.duration(6.0));
                    }
                }
                "modal" => {
                    let entity = world
                        .get_resource::<ignit::ui::UiForest>()
                        .and_then(|forest| forest.find(&"modal".into()));
                    if let Some(entity) = entity
                        && let Some(store) = world.get_component_store_mut::<UiWidget>()
                    {
                        let store = store.get_mut_store();
                        if let Some(widget) = store.get_component_mut(entity.index)
                            && let UiWidgetSpec::Modal { open } = &mut widget.spec
                        {
                            *open = !*open;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn update_graphs(&mut self, world: &mut World) {
        self.phase += 0.06;
        let phase = self.phase;
        for (id, phase_offset) in [
            ("demo_graph_bars", 0.0),
            ("demo_graph_line", 0.7),
            ("demo_graph_area", 1.4),
        ] {
            let Some(entity) = world
                .get_resource::<ignit::ui::UiForest>()
                .and_then(|forest| forest.find(&id.into()))
            else {
                continue;
            };
            let Some(store) = world.get_component_store_mut::<UiWidget>() else {
                continue;
            };
            let Some(widget) = store.get_mut_store().get_component_mut(entity.index) else {
                continue;
            };
            let UiWidgetSpec::Graph { series, .. } = &mut widget.spec else {
                continue;
            };
            for (series_index, series) in series.iter_mut().enumerate() {
                for (index, value) in series.values.iter_mut().enumerate() {
                    *value = demo_graph_value(phase + phase_offset, series_index, index);
                }
            }
        }
    }
}

impl UiSystem for DemoSystem {
    fn name(&self) -> &str {
        "ui_demo_actions"
    }

    fn run(&mut self, world: &mut World) {
        self.apply_events(world);
        self.update_graphs(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showcase_builds_with_unique_ids() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let forest = world.get_resource::<ignit::ui::UiForest>().unwrap();
        assert!(forest.find(&"row_00".into()).is_some());
        assert!(forest.find(&"row_17".into()).is_some());
        assert!(forest.find(&"ron_title".into()).is_some());
        assert!(forest.find(&"demo_graph_bars".into()).is_some());
        assert!(forest.find(&"demo_graph_line".into()).is_some());
        assert!(forest.find(&"demo_graph_area".into()).is_some());
    }

    #[test]
    fn graph_section_does_not_overlap_the_scroll_list() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let mut text = ignit::ui::UiFallbackTextMeasure;
        ignit::ui::update(&mut world, &mut text);

        let forest = world.get_resource::<ignit::ui::UiForest>().unwrap();
        let scroll = forest.find(&"scroll_panel".into()).unwrap();
        let graphs = forest.find(&"demo_graphs".into()).unwrap();
        let computed = world
            .get_component_store::<ignit::ui::UiComputed>()
            .unwrap();
        let scroll = computed.get_component(scroll.index).unwrap().rect;
        let graphs = computed.get_component(graphs.index).unwrap().rect;
        assert!(
            graphs.y >= scroll.y + scroll.height,
            "graphs {:?} overlap scroll panel {:?}",
            graphs,
            scroll
        );
        let ron_root = forest.find(&"ron_root".into()).unwrap();
        let ron = computed.get_component(ron_root.index).unwrap().rect;
        assert!(
            graphs.intersect(ron).is_none(),
            "graphs {:?} overlap the RON panel {:?}",
            graphs,
            ron
        );

        let root = forest.find(&"demo_root".into()).unwrap();
        let layout = world
            .get_component_store::<ignit::ui::UiLayout>()
            .unwrap()
            .get_component(root.index)
            .cloned()
            .unwrap();
        assert_eq!(layout.width, UiDimension::FULL);
        assert_eq!(layout.height, UiDimension::FULL);
    }

    #[test]
    fn maximized_layout_keeps_graphs_ron_and_tabs_reachable() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let roots = world
            .get_resource::<ignit::ui::UiForest>()
            .unwrap()
            .roots()
            .to_vec();
        if let Some(views) = world.get_component_store_mut::<ignit::ui::UiView>() {
            let store = views.get_mut_store();
            for root in roots {
                if let Some(view) = store.get_component_mut(root.index) {
                    view.size = ignit::ui::UiSize::new(1920.0, 1080.0);
                }
            }
        }
        let mut text = ignit::ui::UiFallbackTextMeasure;
        ignit::ui::update(&mut world, &mut text);
        ignit::ui::update(&mut world, &mut text);

        let forest = world.get_resource::<ignit::ui::UiForest>().unwrap();
        let graphs = forest.find(&"demo_graphs".into()).unwrap();
        let ron_root = forest.find(&"ron_root".into()).unwrap();
        let tabs = forest.find(&"tabs".into()).unwrap();
        let computed = world
            .get_component_store::<ignit::ui::UiComputed>()
            .unwrap();
        let graphs = computed.get_component(graphs.index).unwrap().rect;
        let ron = computed.get_component(ron_root.index).unwrap().rect;
        assert!(graphs.intersect(ron).is_none());
        assert!(ron.x + ron.width <= 1920.0);
        assert!(ron.y + ron.height <= 1080.0);

        let draw = world.get_resource::<ignit::ui::UiDrawList>().unwrap();
        assert!(draw.commands.iter().any(|command| matches!(
            command,
            ignit::ui::UiDrawCommand::Text { entity, text, .. }
                if *entity == tabs && text == "Overview"
        )));
    }

    #[test]
    fn tabs_labels_exist_at_the_default_scroll_position() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let mut text = ignit::ui::UiFallbackTextMeasure;
        ignit::ui::update(&mut world, &mut text);

        let forest = world.get_resource::<ignit::ui::UiForest>().unwrap();
        let content = forest.find(&"demo_content".into()).unwrap();
        let scroll = world
            .get_component_store::<ignit::ui::UiScroll>()
            .and_then(|store| store.get_component(content.index).copied())
            .expect("demo content scroll");
        assert_eq!(scroll.offset.y, 0.0);

        let tabs = forest.find(&"tabs".into()).unwrap();
        let draw = world.get_resource::<ignit::ui::UiDrawList>().unwrap();
        let labels = draw
            .commands
            .iter()
            .filter_map(|command| match command {
                ignit::ui::UiDrawCommand::Text { entity, text, .. } if *entity == tabs => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["Overview", "Rendering", "Console"]);
    }

    #[test]
    fn ui_demo_disables_the_world_environment() {
        let mut world = World::new();
        disable_world_environment(&mut world);
        let descriptor = world
            .get_resource::<EnvironmentDescriptorResource>()
            .and_then(|resource| resource.0.clone());
        assert!(matches!(descriptor, Some(WorldEnvironmentDescriptor::None)));
    }

    #[test]
    fn showcase_graphs_animate() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let mut system = DemoSystem::new();
        let graph = world
            .get_resource::<ignit::ui::UiForest>()
            .unwrap()
            .find(&"demo_graph_line".into())
            .unwrap();
        let first = match world
            .get_component_store::<UiWidget>()
            .unwrap()
            .get_component(graph.index)
            .cloned()
            .unwrap()
            .spec
        {
            UiWidgetSpec::Graph { series, .. } => series[0].values[0],
            _ => panic!("demo graph missing"),
        };

        system.update_graphs(&mut world);

        let second = match world
            .get_component_store::<UiWidget>()
            .unwrap()
            .get_component(graph.index)
            .cloned()
            .unwrap()
            .spec
        {
            UiWidgetSpec::Graph { series, .. } => series[0].values[0],
            _ => panic!("demo graph missing"),
        };
        assert_ne!(first, second, "demo graphs animate every frame");
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use ignit::ecs::Events;

    #[test]
    fn theme_action_mutates_theme_tokens() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        install_themes(&mut world);
        let mut system = DemoSystem::new();
        let before = world
            .get_resource::<UiThemes>()
            .unwrap()
            .active
            .color("bg")
            .unwrap();
        if let Some(mut events) = world.get_resource_mut::<Events<UiEvent>>() {
            events.send(UiEvent::Action {
                entity: ignit::ecs::Entity::new(0, 0),
                id: "theme_button".into(),
                action: UiAction::new("theme"),
            });
        }
        system.apply_events(&mut world);
        let after = world
            .get_resource::<UiThemes>()
            .unwrap()
            .active
            .color("bg")
            .unwrap();
        assert_ne!(before, after);
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;

    #[test]
    fn slider_value_updates_progress_bar() {
        let mut world = World::new();
        ignit::ui::init(&mut world);
        build_showcase(&mut world);
        let mut system = DemoSystem::new();
        if let Some(mut events) = world.get_resource_mut::<ignit::ecs::Events<UiEvent>>() {
            events.send(UiEvent::ValueChanged {
                entity: ignit::ecs::Entity::new(0, 0),
                id: "slider".into(),
                value: "0.750".into(),
            });
        }
        system.apply_events(&mut world);
        let progress = world
            .get_resource::<ignit::ui::UiForest>()
            .unwrap()
            .find(&"progress".into())
            .unwrap();
        let widget = world
            .get_component_store::<UiWidget>()
            .unwrap()
            .get_component(progress.index)
            .cloned()
            .unwrap();
        match widget.spec {
            UiWidgetSpec::ProgressBar { value } => assert!((value - 0.75).abs() < f32::EPSILON),
            _ => panic!("progress widget missing"),
        }
    }
}
