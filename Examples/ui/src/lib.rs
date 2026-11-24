use std::sync::{Arc, Mutex};

use ignit::app::{App, AppSettings, Module, UiSystem};
use ignit::cgmath::{Point3, Rad};
use ignit::ecs::{System, World};
use ignit::ecs_bridge::{ActiveCamera, CameraDescriptor, Position, Rotation};
use ignit::logging::{self, error, info};
use ignit::ui::{
    UiAction, UiBuilder, UiColor, UiDimension, UiDocument, UiDocumentNode, UiEdges, UiEvent,
    UiLayout, UiLengthPercentage, UiStyle, UiTextStyle, UiTextWrap, UiTheme, UiThemes, UiView,
    UiWidget, UiWidgetSpec,
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

fn install_themes(world: &mut World) {
    let dark = UiTheme::dark();
    let light = UiTheme::light();
    let mut themes = UiThemes::with_active(dark.clone());
    themes.named.insert("dark".into(), dark);
    themes.named.insert("light".into(), light);
    world.insert_resource(themes);
}

fn build_showcase(world: &mut World) {
    let mut root = UiDocumentNode::new("demo_root")
        .layout(
            UiLayout::column()
                .size(1280.0, 720.0)
                .padding(24.0)
                .gap(12.0),
        )
        .style(
            UiStyle::default()
                .background(UiColor::Token("bg".into()))
                .class("panel"),
        )
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
                .layout(UiLayout::default().width(420.0).height(44.0))
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
                .layout(UiLayout::column().height(44.0).padding(8.0))
                .style(UiStyle::default().class("panel"))
                .child(UiDocumentNode::new("tab_overview_text").text("Overview content")),
        )
        .child(
            UiDocumentNode::new("tab_rendering")
                .layout(UiLayout::column().height(44.0).padding(8.0))
                .style(UiStyle::default().class("panel").visible(false))
                .child(UiDocumentNode::new("tab_rendering_text").text("Rendering content")),
        )
        .child(
            UiDocumentNode::new("tab_console")
                .layout(UiLayout::column().height(44.0).padding(8.0))
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
                        .width(520.0)
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
        .child(
            UiDocumentNode::new("floating")
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
                            left: UiDimension::Length(940.0),
                            right: UiDimension::Auto,
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
                ),
        )
        .child(
            UiDocumentNode::new("modal")
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
                ),
        );
    root.view = Some(UiView::new(1280.0, 720.0));
    UiBuilder::new().root(root).build(world).unwrap();

    let ron = include_str!("../Assets/Scenes/ui_demo.ron");
    if let Ok(document) = UiDocument::from_ron(ron) {
        let _ = ignit::ui::spawn_document(world, &document, Some("ui_demo"));
    }
}

struct DemoSystem {
    dark: bool,
}

impl DemoSystem {
    fn new() -> Self {
        Self { dark: true }
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
}

impl UiSystem for DemoSystem {
    fn name(&self) -> &str {
        "ui_demo_actions"
    }

    fn run(&mut self, world: &mut World) {
        self.apply_events(world);
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
            events.send(UiEvent::Click {
                entity: ignit::ecs::Entity::new(0, 0),
                id: "theme_button".into(),
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
