//! Code-authored UI and runtime handles.

use ignit_ecs::{Entity, World};

use crate::document::{UiDocument, UiDocumentNode, spawn_document};
use crate::focus;
use crate::id::UiId;
use crate::layout::UiLayout;
use crate::node::{UiDirty, UiInteraction, UiScroll, UiView};
use crate::style::UiStyle;
use crate::style::UiText;

/// Code-authored document builder.
#[derive(Clone, Debug, Default)]
pub struct UiBuilder {
    document: UiDocument,
}

impl UiBuilder {
    pub fn new() -> Self {
        Self {
            document: UiDocument::default(),
        }
    }

    pub fn root(mut self, root: UiDocumentNode) -> Self {
        self.document.roots.push(root);
        self
    }

    pub fn build(self, world: &mut World) -> Result<Vec<UiHandle>, String> {
        let roots = spawn_document(world, &self.document, None)?;
        let ids: Vec<UiId> = self
            .document
            .roots
            .iter()
            .map(|root| root.id.clone())
            .collect();
        Ok(roots
            .into_iter()
            .zip(ids)
            .map(|(entity, id)| UiHandle { entity, id })
            .collect())
    }
}

/// Runtime handle to a spawned UI node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiHandle {
    pub entity: Entity,
    pub id: UiId,
}

impl UiHandle {
    pub fn find(world: &World, id: &UiId) -> Option<Self> {
        let entity = world
            .get_resource::<crate::forest::UiForest>()
            .and_then(|forest| forest.find(id))?;
        Some(Self {
            entity,
            id: id.clone(),
        })
    }

    pub fn set_text(&self, world: &mut World, value: impl Into<String>) {
        let _ = world.attach_component(
            &self.entity,
            UiText {
                value: value.into(),
            },
        );
        mark(&self.entity, world, UiDirty::TEXT | UiDirty::LAYOUT);
    }

    pub fn set_layout(&self, world: &mut World, layout: UiLayout) {
        let _ = world.attach_component(&self.entity, layout);
        mark(&self.entity, world, UiDirty::LAYOUT);
    }

    pub fn set_style(&self, world: &mut World, style: UiStyle) {
        let _ = world.attach_component(&self.entity, style);
        mark(&self.entity, world, UiDirty::LAYOUT | UiDirty::PAINT);
    }

    pub fn update_style(&self, world: &mut World, update: impl FnOnce(&mut UiStyle)) {
        let Some(mut style) = world
            .get_component_store::<UiStyle>()
            .and_then(|store| store.get_component(self.entity.index).cloned())
        else {
            return;
        };
        update(&mut style);
        self.set_style(world, style);
    }

    pub fn set_view(&self, world: &mut World, view: UiView) {
        let _ = world.attach_component(&self.entity, view);
        mark(&self.entity, world, UiDirty::LAYOUT);
    }

    pub fn set_visible(&self, world: &mut World, visible: bool) {
        self.update_style(world, |style| style.visible = Some(visible));
    }

    pub fn set_scroll(&self, world: &mut World, scroll: UiScroll) {
        let _ = world.attach_component(&self.entity, scroll);
        mark(&self.entity, world, UiDirty::LAYOUT | UiDirty::PAINT);
    }

    pub fn scroll(&self, world: &World) -> Option<UiScroll> {
        world
            .get_component_store::<UiScroll>()
            .and_then(|store| store.get_component(self.entity.index).copied())
    }

    pub fn interaction(&self, world: &World) -> Option<UiInteraction> {
        world
            .get_component_store::<UiInteraction>()
            .and_then(|store| store.get_component(self.entity.index).copied())
    }

    pub fn focus(&self, world: &mut World) {
        focus::focus_entity(world, self.entity);
    }
}

fn mark(entity: &Entity, world: &mut World, dirty: UiDirty) {
    if let Some(store) = world.get_component_store_mut::<UiDirty>() {
        let store = store.get_mut_store();
        if let Some(current) = store.get_component(entity.index) {
            let next = *current | dirty;
            store.attach(entity.index, next);
        } else {
            store.attach(entity.index, dirty);
        }
    }
}
