//! Resolves theme tokens, classes, and state into a concrete style.

use ignit_ecs::{Entity, World};

use crate::node::{UiDirty, UiInteraction, UiNode, UiThemeScope};
use crate::style::{UiStyle, UiThemes};

/// Resolves every node's style. Returns the number of nodes updated.
pub fn resolve_styles(world: &mut World) -> usize {
    let entities: Vec<Entity> = world.entities().collect();
    let mut updated = 0;
    for entity in entities {
        let Some(style) = world
            .get_component_store::<UiStyle>()
            .and_then(|store| store.get_component(entity.index).cloned())
        else {
            continue;
        };
        let (theme_name, inherited) = scope_for(world, entity);
        let state = world
            .get_component_store::<UiInteraction>()
            .and_then(|store| {
                store
                    .get_component(entity.index)
                    .map(|interaction| interaction.state())
            })
            .unwrap_or_default();

        let resolved = {
            let Some(themes) = world.get_resource::<UiThemes>() else {
                continue;
            };
            let theme = themes.get(theme_name.as_deref());
            theme.resolve_style(&style, &inherited, state)
        };
        world
            .attach_component(&entity, resolved)
            .expect("valid ui entity");
        updated += 1;
    }
    clear_paint_dirty(world);
    updated
}

fn scope_for(world: &World, entity: Entity) -> (Option<String>, Vec<String>) {
    let nodes = world.get_component_store::<UiNode>();
    let scopes = world.get_component_store::<UiThemeScope>();
    let mut current = Some(entity);
    let mut theme = None;
    let mut classes = Vec::new();
    while let Some(entity) = current {
        if let Some(scope) = scopes
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
        {
            if theme.is_none() {
                theme = scope.theme.clone();
            }
            classes.splice(0..0, scope.classes.iter().cloned());
        }
        current = nodes
            .as_ref()
            .and_then(|store| store.get_component(entity.index))
            .and_then(|node| node.parent);
    }
    (theme, classes)
}

fn clear_paint_dirty(world: &mut World) {
    let entities: Vec<Entity> = world.entities().collect();
    if let Some(store) = world.get_component_store_mut::<UiDirty>() {
        let store = store.get_mut_store();
        for entity in entities {
            if let Some(dirty) = store.get_component_mut(entity.index) {
                dirty.remove(UiDirty::PAINT);
            }
        }
    }
}
