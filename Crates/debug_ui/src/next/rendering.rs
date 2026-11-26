//! New-UI rendering controls panel.

use ignit_app::RenderSettings;
use ignit_ecs::{Entity, World};
use ignit_ecs_bridge::VolumetricSettings;
use ignit_renderer::{AntiAliasing, DebugView, SmaaQuality};
use ignit_ui::{UiDocumentNode, UiEvent, UiLayout, UiStyle, UiTextStyle, UiWidget, UiWidgetSpec};

use crate::state::{DebugMessage, DebugUiState, PanelId};

const WINDOW_ID: &str = "debug_render_window";
const AA_OPTIONS: [AntiAliasing; 3] = [AntiAliasing::Off, AntiAliasing::Fxaa, AntiAliasing::Smaa];
const SMAA_OPTIONS: [SmaaQuality; 4] = [
    SmaaQuality::Low,
    SmaaQuality::Medium,
    SmaaQuality::High,
    SmaaQuality::Ultra,
];

pub(super) fn build() -> UiDocumentNode {
    let mut content = UiDocumentNode::new("debug_render_scroll").layout(
        UiLayout::column()
            .absolute()
            .inset(ignit_ui::UiEdges {
                left: ignit_ui::UiDimension::Length(8.0),
                right: ignit_ui::UiDimension::Length(8.0),
                top: ignit_ui::UiDimension::Length(34.0),
                bottom: ignit_ui::UiDimension::Length(8.0),
            })
            .gap(4.0),
    );
    content = content.scroll();

    content = content.child(heading("Anti-aliasing"));
    content = content.child(dropdown(
        "debug_render_aa",
        &AA_OPTIONS
            .iter()
            .map(|mode| label_aa(*mode))
            .collect::<Vec<_>>(),
        0,
    ));
    content = content.child(dropdown(
        "debug_render_smaa",
        &SMAA_OPTIONS
            .iter()
            .map(|q| label_smaa(*q))
            .collect::<Vec<_>>(),
        2,
    ));
    content = content.child(heading("Debug view"));
    content = content.child(dropdown(
        "debug_render_view",
        &DebugView::ALL.iter().map(|v| v.label()).collect::<Vec<_>>(),
        0,
    ));
    content = content.child(heading("Scaling"));
    content = content.child(slider("Render scale", "debug_render_scale", 0.25, 1.5));
    content = content.child(toggle_row("Adaptive scale", "debug_render_adaptive"));
    content = content.child(heading("Tonemap"));
    content = content.child(slider("Exposure", "debug_render_exposure", 0.1, 4.0));
    content = content.child(toggle_row("Bloom", "debug_render_bloom"));
    content = content.child(slider(
        "Bloom threshold",
        "debug_render_bloom_threshold",
        0.0,
        4.0,
    ));
    content = content.child(slider(
        "Bloom intensity",
        "debug_render_bloom_intensity",
        0.0,
        4.0,
    ));
    content = content.child(toggle_row("Deferred", "debug_render_deferred"));
    content = content.child(heading("Volumetrics"));
    content = content.child(toggle_row("Enabled", "debug_render_volumetrics"));
    content = content.child(slider("Fog density", "debug_render_density", 0.0, 0.2));

    UiDocumentNode::new(WINDOW_ID)
        .layout(
            UiLayout::column()
                .width(320.0)
                .height(440.0)
                .absolute()
                .inset(ignit_ui::UiEdges {
                    left: ignit_ui::UiDimension::Length(960.0),
                    right: ignit_ui::UiDimension::Auto,
                    top: ignit_ui::UiDimension::Length(320.0),
                    bottom: ignit_ui::UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "Rendering".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel").padding_text(UiTextStyle {
            size: 12.0,
            ..Default::default()
        }))
        .child(content)
        .child(super::close_button(
            "debug_render_close",
            "debug_render_hide",
        ))
}

fn heading(text: &str) -> UiDocumentNode {
    UiDocumentNode::new(format!("debug_render_heading_{text}"))
        .text(text)
        .style(UiStyle::default().padding_text(UiTextStyle {
            size: 13.0,
            weight: 600,
            ..Default::default()
        }))
}

fn dropdown(id: &str, options: &[&str], selected: usize) -> UiDocumentNode {
    UiDocumentNode::new(id)
        .layout(UiLayout::row().width(280.0).height(38.0))
        .widget(UiWidgetSpec::Dropdown {
            options: options.iter().map(|value| (*value).to_string()).collect(),
            selected,
            open: false,
        })
        .focusable()
        .style(UiStyle::default().class("panel").padding_text(UiTextStyle {
            size: 12.0,
            ..Default::default()
        }))
}

fn toggle_row(label: &str, id: &str) -> UiDocumentNode {
    UiDocumentNode::new(format!("{id}_row"))
        .layout(UiLayout::row().width(280.0).height(24.0).gap(6.0))
        .child(
            UiDocumentNode::new(format!("{id}_label"))
                .layout(UiLayout {
                    flex_grow: 1.0,
                    ..Default::default()
                })
                .text(label),
        )
        .child(
            UiDocumentNode::new(id)
                .layout(UiLayout::default().size(40.0, 22.0))
                .widget(UiWidgetSpec::Toggle { on: false }),
        )
}

fn slider(label: &str, id: &str, min: f32, max: f32) -> UiDocumentNode {
    UiDocumentNode::new(format!("{id}_row"))
        .layout(UiLayout::row().width(280.0).height(24.0).gap(6.0))
        .child(
            UiDocumentNode::new(format!("{id}_label"))
                .layout(UiLayout::default().width(128.0))
                .text(format!("{label}: --")),
        )
        .child(
            UiDocumentNode::new(id)
                .layout(UiLayout {
                    flex_grow: 1.0,
                    height: ignit_ui::UiDimension::Length(22.0),
                    ..Default::default()
                })
                .widget(UiWidgetSpec::Slider {
                    value: min,
                    min,
                    max,
                })
                .focusable(),
        )
}

pub(super) fn handle_event(world: &mut World, event: &UiEvent) -> bool {
    match event {
        UiEvent::ValueChanged { id, value, .. } => {
            let id = id.as_str();
            match id {
                "debug_render_scale" => parse_f32(value).map(DebugMessage::SetRenderScale),
                "debug_render_adaptive" => {
                    parse_bool(value).map(DebugMessage::SetAdaptiveRenderScale)
                }
                "debug_render_exposure" => parse_f32(value).map(DebugMessage::SetExposure),
                "debug_render_bloom" => parse_bool(value).map(DebugMessage::SetBloomEnabled),
                "debug_render_bloom_threshold" => {
                    parse_f32(value).map(DebugMessage::SetBloomThreshold)
                }
                "debug_render_bloom_intensity" => {
                    parse_f32(value).map(DebugMessage::SetBloomIntensity)
                }
                "debug_render_deferred" => parse_bool(value).map(DebugMessage::SetDeferredEnabled),
                "debug_render_volumetrics" => {
                    parse_bool(value).map(DebugMessage::SetVolumetricsEnabled)
                }
                "debug_render_density" => parse_f32(value).map(DebugMessage::SetVolumetricDensity),
                _ => None,
            }
            .map(|message| {
                crate::handle_debug_message_world(message, world);
                true
            })
            .unwrap_or(false)
        }
        UiEvent::Action { action, .. } if action.name == "debug_render_hide" => {
            crate::handle_debug_message_world(DebugMessage::HidePanel(PanelId::Rendering), world);
            true
        }
        _ => false,
    }
}

fn parse_f32(value: &str) -> Option<f32> {
    value.parse().ok()
}

fn parse_bool(value: &str) -> Option<bool> {
    value.parse().ok()
}

fn label_aa(mode: AntiAliasing) -> &'static str {
    match mode {
        AntiAliasing::Off => "Off",
        AntiAliasing::Fxaa => "FXAA",
        AntiAliasing::Smaa => "SMAA",
    }
}

fn label_smaa(quality: SmaaQuality) -> &'static str {
    match quality {
        SmaaQuality::Low => "Low",
        SmaaQuality::Medium => "Medium",
        SmaaQuality::High => "High",
        SmaaQuality::Ultra => "Ultra",
    }
}

pub(super) fn refresh(world: &mut World) {
    let settings = world
        .get_resource::<RenderSettings>()
        .map(|settings| *settings)
        .unwrap_or_default();
    let volumetrics = world
        .get_resource::<VolumetricSettings>()
        .map(|settings| *settings)
        .unwrap_or_default();

    set_slider(world, "debug_render_scale", settings.render_scale);
    super::set_text(
        world,
        "debug_render_scale_label",
        format!("Render scale: {:.2}", settings.render_scale),
    );
    set_slider(world, "debug_render_exposure", settings.exposure);
    super::set_text(
        world,
        "debug_render_exposure_label",
        format!("Exposure: {:.2}", settings.exposure),
    );
    set_slider(
        world,
        "debug_render_bloom_threshold",
        settings.bloom_threshold,
    );
    super::set_text(
        world,
        "debug_render_bloom_threshold_label",
        format!("Bloom threshold: {:.2}", settings.bloom_threshold),
    );
    set_slider(
        world,
        "debug_render_bloom_intensity",
        settings.bloom_intensity,
    );
    super::set_text(
        world,
        "debug_render_bloom_intensity_label",
        format!("Bloom intensity: {:.2}", settings.bloom_intensity),
    );
    set_slider(world, "debug_render_density", volumetrics.density);
    super::set_text(
        world,
        "debug_render_density_label",
        format!("Fog density: {:.3}", volumetrics.density),
    );
    set_toggle(
        world,
        "debug_render_adaptive",
        settings.adaptive_render_scale,
    );
    set_toggle(world, "debug_render_bloom", settings.bloom_enabled);
    set_toggle(world, "debug_render_deferred", settings.deferred_enabled);
    set_toggle(world, "debug_render_volumetrics", volumetrics.enabled);

    let aa_index = AA_OPTIONS
        .iter()
        .position(|mode| *mode == settings.anti_aliasing)
        .unwrap_or(0);
    poll_dropdown(world, "debug_render_aa", aa_index, |index| {
        Some(DebugMessage::SetAntiAliasing(AA_OPTIONS[index]))
    });
    let smaa_index = SMAA_OPTIONS
        .iter()
        .position(|quality| *quality == settings.smaa_quality)
        .unwrap_or(2);
    poll_dropdown(world, "debug_render_smaa", smaa_index, |index| {
        Some(DebugMessage::SetSmaaQuality(SMAA_OPTIONS[index]))
    });
    let view_index = DebugView::ALL
        .iter()
        .position(|view| *view == settings.debug_view)
        .unwrap_or(0);
    poll_dropdown(world, "debug_render_view", view_index, |index| {
        Some(DebugMessage::SetDebugView(DebugView::ALL[index]))
    });

    let open = world
        .get_resource::<DebugUiState>()
        .map(|state| state.is_panel_visible(PanelId::Rendering))
        .unwrap_or(false);
    if let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&WINDOW_ID.into()))
    {
        super::set_panel_open(world, entity, open);
    }
}

fn set_slider(world: &mut World, id: &str, value: f32) {
    let Some(entity) = find(world, id) else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Slider { value: current, .. } = &mut widget.spec
        && (*current - value).abs() > 1.0e-4
    {
        *current = value;
        changed = true;
    }
    if changed {
        super::mark_dirty(world, entity, ignit_ui::UiDirty::PAINT);
    }
}

fn set_toggle(world: &mut World, id: &str, on: bool) {
    let Some(entity) = find(world, id) else {
        return;
    };
    let mut changed = false;
    if let Some(store) = world.get_component_store_mut::<UiWidget>()
        && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
        && let UiWidgetSpec::Toggle { on: current } = &mut widget.spec
        && *current != on
    {
        *current = on;
        changed = true;
    }
    if changed {
        super::mark_dirty(world, entity, ignit_ui::UiDirty::PAINT);
    }
}

fn poll_dropdown(
    world: &mut World,
    id: &str,
    expected: usize,
    message: impl Fn(usize) -> Option<DebugMessage>,
) {
    let Some(entity) = find(world, id) else {
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
        && let Some(message) = message(selected)
    {
        crate::handle_debug_message_world(message, world);
    }
}

fn find(world: &World, id: &str) -> Option<Entity> {
    world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&id.into()))
}
