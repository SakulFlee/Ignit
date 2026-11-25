//! New-UI console panel.

use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiColor, UiDimension, UiDocumentNode, UiEdges, UiLayout, UiLengthPercentage, UiStyle,
    UiTextStyle, UiWidget, UiWidgetSpec,
};
use log::LevelFilter;

use crate::state::{DebugUiState, MAX_ROWS};

const ROWS: usize = 80;

const WINDOW_ID: &str = "debug_next_console_window";
const ROW_PREFIX: &str = "debug_console_row_";

pub(super) fn build() -> UiDocumentNode {
    let mut rows = UiDocumentNode::new("debug_console_scroll").layout(UiLayout {
        flex_grow: 1.0,
        min_height: UiDimension::Length(0.0),
        ..UiLayout::column().width_full().gap(4.0)
    });
    rows = rows.scroll();
    for index in 0..ROWS {
        rows = rows.child(
            UiDocumentNode::new(format!("{ROW_PREFIX}{index}"))
                .layout(UiLayout::default().width_full().shrink(0.0))
                .style(UiStyle {
                    visible: Some(false),
                    ..UiStyle::default()
                }),
        );
    }

    let content = UiDocumentNode::new("debug_console_body")
        .layout(
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
        )
        .child(header())
        .child(rows);

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
        .child(super::close_button(
            "debug_console_close",
            "debug_console_hide",
        ))
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
}

pub(super) fn levels() -> [(&'static str, LevelFilter); 5] {
    [
        ("Error", LevelFilter::Error),
        ("Warn", LevelFilter::Warn),
        ("Info", LevelFilter::Info),
        ("Debug", LevelFilter::Debug),
        ("Trace", LevelFilter::Trace),
    ]
}

pub(super) fn refresh(world: &mut World) {
    let min_level = world
        .get_resource::<DebugUiState>()
        .map_or(LevelFilter::Info, |state| state.min_level);

    let mut lines = Vec::with_capacity(ROWS);
    ignit_core::logging::with_log_buffer(|buffer| {
        for line in buffer
            .rev()
            .filter(|line| line.level <= min_level)
            .take(ROWS)
        {
            lines.push((line.level, line.target.clone(), line.message.clone()));
        }
    });

    for index in 0..ROWS {
        let id = format!("{ROW_PREFIX}{index}");
        let Some(entity) = world
            .get_resource::<ignit_ui::UiForest>()
            .and_then(|forest| forest.find(&id.as_str().into()))
        else {
            continue;
        };
        if let Some((level, target, message)) = lines.get(index) {
            let message = message.replace(['\n', '\r'], " ");
            let preview: String = if message.chars().count() > 180 {
                message.chars().take(177).collect::<String>() + "..."
            } else {
                message
            };
            super::set_text(world, &id, format!("[{level}] {target}: {preview}"));
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
        super::set_visible(world, entity, open);
    }
}

fn set_row_style(world: &mut World, entity: Entity, color: Option<UiColor>) {
    let style = match color {
        Some(UiColor::Rgba(color)) => UiStyle::default()
            .background(UiColor::Rgba([color[0], color[1], color[2], 0.12]))
            .padding_text(UiTextStyle {
                size: 10.0,
                color: UiColor::Rgba(color),
                wrap: ignit_ui::UiTextWrap::NoWrap,
                ..Default::default()
            }),
        Some(color) => UiStyle::default().padding_text(UiTextStyle {
            size: 10.0,
            color,
            wrap: ignit_ui::UiTextWrap::NoWrap,
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
