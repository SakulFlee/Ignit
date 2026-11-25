//! New-UI console panel.

use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiColor, UiDimension, UiDocumentNode, UiEdges, UiLayout, UiLengthPercentage, UiStyle,
    UiTextStyle, UiWidget, UiWidgetSpec,
};
use log::LevelFilter;

use crate::state::{DebugUiState, MAX_ROWS};

const WINDOW_ID: &str = "debug_next_console_window";
const ROW_PREFIX: &str = "debug_console_row_";

pub(super) fn build() -> UiDocumentNode {
    let mut content = UiDocumentNode::new("debug_console_scroll").layout(
        UiLayout::column()
            .width_full()
            .height_full()
            .padding_edges(UiEdges {
                left: UiLengthPercentage::Length(10.0),
                right: UiLengthPercentage::Length(10.0),
                top: UiLengthPercentage::Length(34.0),
                bottom: UiLengthPercentage::Length(10.0),
            })
            .gap(4.0),
    );
    content = content.scroll().child(header());

    for index in 0..MAX_ROWS {
        content = content.child(
            UiDocumentNode::new(format!("{ROW_PREFIX}{index}"))
                .layout(UiLayout::default().width_full())
                .style(UiStyle {
                    visible: Some(false),
                    ..UiStyle::default()
                }),
        );
    }

    UiDocumentNode::new(WINDOW_ID)
        .layout(
            UiLayout::column()
                .width(760.0)
                .height(260.0)
                .absolute()
                .inset(UiEdges {
                    left: UiDimension::Length(500.0),
                    right: UiDimension::Auto,
                    top: UiDimension::Length(40.0),
                    bottom: UiDimension::Auto,
                }),
        )
        .widget(UiWidgetSpec::FloatingWindow {
            title: "Console".into(),
            open: true,
        })
        .style(UiStyle::default().class("panel"))
        .child(content)
}

fn header() -> UiDocumentNode {
    let mut row = UiDocumentNode::new("debug_console_header").layout(
        UiLayout::row()
            .width_full()
            .height(26.0)
            .gap(4.0)
            .align_items(ignit_ui::UiAlign::Center),
    );
    for (index, (label, _)) in levels().iter().enumerate() {
        row = row.child(
            UiDocumentNode::new(format!("debug_console_level_{index}"))
                .layout(UiLayout::default().size(58.0, 24.0))
                .text(*label)
                .widget(UiWidgetSpec::Button {
                    label: (*label).into(),
                })
                .action(ignit_ui::UiAction::new(format!(
                    "debug_console_level_{index}"
                )))
                .focusable()
                .style(UiStyle::default().class("panel")),
        );
    }
    row.child(
        UiDocumentNode::new("debug_console_clear")
            .layout(UiLayout::default().size(58.0, 24.0))
            .text("Clear")
            .widget(UiWidgetSpec::Button {
                label: "Clear".into(),
            })
            .action(ignit_ui::UiAction::new("debug_console_clear"))
            .focusable()
            .style(UiStyle::default().class("panel")),
    )
    .child(
        UiDocumentNode::new("debug_console_hide")
            .layout(UiLayout::default().size(58.0, 24.0))
            .text("Hide")
            .widget(UiWidgetSpec::Button {
                label: "Hide".into(),
            })
            .action(ignit_ui::UiAction::new("debug_console_hide"))
            .focusable()
            .style(UiStyle::default().class("panel")),
    )
}

pub(super) fn levels() -> [(&'static str, LevelFilter); 5] {
    [
        ("Off", LevelFilter::Off),
        ("Error", LevelFilter::Error),
        ("Warn", LevelFilter::Warn),
        ("Info", LevelFilter::Info),
        ("Debug", LevelFilter::Debug),
    ]
}

pub(super) fn refresh(world: &mut World) {
    let min_level = world
        .get_resource::<DebugUiState>()
        .map_or(LevelFilter::Info, |state| state.min_level);

    let mut lines = Vec::with_capacity(MAX_ROWS);
    ignit_core::logging::with_log_buffer(|buffer| {
        for line in buffer
            .rev()
            .filter(|line| line.level <= min_level)
            .take(MAX_ROWS)
        {
            lines.push((line.level, line.target.clone(), line.message.clone()));
        }
    });

    for index in 0..MAX_ROWS {
        let id = format!("{ROW_PREFIX}{index}");
        let Some(entity) = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&id.as_str().into()))
        else {
            continue;
        };
        if let Some((level, target, message)) = lines.get(index) {
            super::set_text(world, &id, format!("[{level}] {target}: {message}"));
            set_row_style(world, entity, Some(level_color(*level)));
        } else {
            set_row_style(world, entity, None);
        }
    }

    let open = world
        .get_resource::<DebugUiState>()
        .map(|state| state.is_panel_visible(crate::state::PanelId::Console))
        .unwrap_or(false);
    if let Some(entity) = world
        .get_resource::<ignit_ui::UiForest>()
        .and_then(|forest| forest.find(&WINDOW_ID.into()))
    {
        let mut changed = false;
        if let Some(store) = world.get_component_store_mut::<UiWidget>()
            && let Some(widget) = store.get_mut_store().get_component_mut(entity.index)
            && let UiWidgetSpec::FloatingWindow { open: value, .. } = &mut widget.spec
            && *value != open
        {
            *value = open;
            changed = true;
        }
        if changed {
            super::mark_dirty(
                world,
                entity,
                ignit_ui::UiDirty::LAYOUT | ignit_ui::UiDirty::PAINT,
            );
        }
    }
}

fn set_row_style(world: &mut World, entity: Entity, color: Option<UiColor>) {
    let style = match color {
        Some(color) => UiStyle::default().padding_text(UiTextStyle {
            size: 11.0,
            color,
            ..Default::default()
        }),
        None => UiStyle {
            visible: Some(false),
            ..UiStyle::default()
        },
    };
    let current = world
        .get_component_store::<UiStyle>()
        .and_then(|store| store.get_component(entity.index).cloned());
    if current.as_ref() != Some(&style) {
        let _ = world.attach_component(&entity, style);
        super::mark_dirty(
            world,
            entity,
            ignit_ui::UiDirty::LAYOUT | ignit_ui::UiDirty::PAINT,
        );
    }
}

fn level_color(level: log::Level) -> UiColor {
    UiColor::Rgba(match level {
        log::Level::Error => [1.0, 0.45, 0.45, 1.0],
        log::Level::Warn => [1.0, 0.8, 0.4, 1.0],
        log::Level::Info => [0.85, 0.9, 0.95, 1.0],
        log::Level::Debug => [0.7, 0.75, 0.8, 1.0],
        log::Level::Trace => [0.55, 0.6, 0.65, 1.0],
    })
}
