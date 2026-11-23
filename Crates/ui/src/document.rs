//! Versioned RON documents, spawning, hot reload, and state restoration.

use std::collections::HashMap;
use std::time::SystemTime;

use ignit_ecs::{Entity, World};
use serde::{Deserialize, Serialize};

use crate::forest::UiForest;
use crate::id::UiId;
use crate::layout::UiLayout;
use crate::node::{
    UiAction, UiDirty, UiFocusable, UiInteraction, UiNode, UiRoot, UiScroll, UiScrollContainer,
    UiThemeScope, UiView,
};
use crate::style::{UiImage, UiStyle, UiText};

pub const UI_DOCUMENT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiDocument {
    pub version: u32,
    pub roots: Vec<UiDocumentNode>,
}

impl Default for UiDocument {
    fn default() -> Self {
        Self {
            version: UI_DOCUMENT_VERSION,
            roots: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiDocumentNode {
    pub id: UiId,
    pub layout: UiLayout,
    pub style: UiStyle,
    pub text: Option<UiText>,
    pub image: Option<UiImage>,
    pub widget: Option<crate::widgets::UiWidgetSpec>,
    pub children: Vec<UiDocumentNode>,
    pub focusable: Option<UiFocusable>,
    pub action: Option<UiAction>,
    pub scroll: bool,
    pub theme_scope: Option<UiThemeScope>,
    /// Optional root viewport; ignored for non-root nodes.
    pub view: Option<UiView>,
}

impl UiDocumentNode {
    pub fn new(id: impl Into<UiId>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }

    pub fn layout(mut self, layout: UiLayout) -> Self {
        self.layout = layout;
        self
    }

    pub fn style(mut self, style: UiStyle) -> Self {
        self.style = style;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(UiText::new(text));
        self
    }

    pub fn child(mut self, child: UiDocumentNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn focusable(mut self) -> Self {
        self.focusable = Some(UiFocusable::default());
        self
    }

    pub fn action(mut self, action: UiAction) -> Self {
        self.action = Some(action);
        self
    }

    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }

    pub fn widget(mut self, widget: crate::widgets::UiWidgetSpec) -> Self {
        self.widget = Some(widget);
        self
    }
}

impl UiDocument {
    pub fn from_ron(source: &str) -> Result<Self, String> {
        let document: Self = ron::from_str(source).map_err(|error| error.to_string())?;
        if document.version > UI_DOCUMENT_VERSION {
            return Err(format!(
                "unsupported UI document version {} (max {})",
                document.version, UI_DOCUMENT_VERSION
            ));
        }
        Ok(document)
    }

    pub fn to_ron(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())
    }

    /// Checks that every ID is present and unique.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashMap::new();
        for root in &self.roots {
            validate_node(root, &mut ids)?;
        }
        Ok(())
    }
}

fn validate_node(node: &UiDocumentNode, ids: &mut HashMap<UiId, ()>) -> Result<(), String> {
    if node.id.is_empty() {
        return Err("UI document contains an empty node ID".to_string());
    }
    if ids.insert(node.id.clone(), ()).is_some() {
        return Err(format!("duplicate UI node ID '{}'", node.id));
    }
    for child in &node.children {
        validate_node(child, ids)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct UiDocumentEntry {
    pub path: String,
    pub document: UiDocument,
    pub modified: Option<SystemTime>,
    pub roots: Vec<Entity>,
}

#[derive(Default)]
pub struct UiDocumentRegistry {
    pub entries: HashMap<String, UiDocumentEntry>,
}

impl UiDocumentRegistry {
    pub fn get(&self, name: &str) -> Option<&UiDocumentEntry> {
        self.entries.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn remove(&mut self, name: &str) -> Option<UiDocumentEntry> {
        self.entries.remove(name)
    }
}

/// Loads a RON document from the asset root and spawns it.
pub fn load_document(
    world: &mut World,
    name: impl Into<String>,
    path: impl Into<String>,
) -> Result<Vec<Entity>, String> {
    let name = name.into();
    let path = path.into();
    let source = ignit_file_manager::FileManager::global()
        .map_err(|error| error.to_string())?
        .read_asset_to_string(&path)
        .map_err(|error| error.to_string())?;
    let document = UiDocument::from_ron(&source)?;
    document.validate()?;
    let roots = spawn_document(world, &document, Some(&name))?;
    let modified = ignit_file_manager::FileManager::global()
        .ok()
        .and_then(|manager| manager.asset_modified(&path));
    ensure_registry(world);
    if let Some(mut registry) = world.get_resource_mut::<UiDocumentRegistry>() {
        registry.entries.insert(
            name,
            UiDocumentEntry {
                path,
                document,
                modified,
                roots: roots.clone(),
            },
        );
    }
    Ok(roots)
}

/// Spawns an already-parsed document.
pub fn spawn_document(
    world: &mut World,
    document: &UiDocument,
    document_name: Option<&str>,
) -> Result<Vec<Entity>, String> {
    document.validate()?;
    ensure_forest(world);
    let mut roots = Vec::new();
    for root in &document.roots {
        let entity = spawn_node(world, root, None);
        world
            .attach_component(&entity, root.view.unwrap_or_default())
            .map_err(|error| format!("{error:?}"))?;
        world
            .attach_component(&entity, UiRoot)
            .map_err(|error| format!("{error:?}"))?;
        roots.push(entity);
    }
    if let Some(mut forest) = world.get_resource_mut::<UiForest>() {
        for root in &roots {
            forest.add_root(*root, document_name);
        }
    }
    Ok(roots)
}

fn spawn_node(world: &mut World, spec: &UiDocumentNode, parent: Option<Entity>) -> Entity {
    let entity = world.spawn_entity();
    let mut node = UiNode::new(spec.id.clone());
    node.parent = parent;
    world
        .attach_component(&entity, node)
        .expect("valid spawned ui entity");
    world
        .attach_component(&entity, spec.layout.clone())
        .expect("valid spawned ui entity");
    world
        .attach_component(&entity, spec.style.clone())
        .expect("valid spawned ui entity");
    world
        .attach_component(&entity, UiDirty::ALL)
        .expect("valid spawned ui entity");
    world
        .attach_component(&entity, UiInteraction::default())
        .expect("valid spawned ui entity");
    if let Some(text) = &spec.text {
        world
            .attach_component(&entity, text.clone())
            .expect("valid spawned ui entity");
    }
    if let Some(image) = &spec.image {
        world
            .attach_component(&entity, image.clone())
            .expect("valid spawned ui entity");
    }
    if let Some(widget) = &spec.widget {
        world
            .attach_component(&entity, crate::widgets::UiWidget::new(widget.clone()))
            .expect("valid spawned ui entity");
        if let crate::widgets::UiWidgetSpec::TextBox { value, .. } = widget
            && spec.text.is_none()
        {
            world
                .attach_component(
                    &entity,
                    UiText {
                        value: value.clone(),
                    },
                )
                .expect("valid spawned ui entity");
        }
    }
    if let Some(focusable) = spec.focusable {
        world
            .attach_component(&entity, focusable)
            .expect("valid spawned ui entity");
    }
    if let Some(action) = &spec.action {
        world
            .attach_component(&entity, action.clone())
            .expect("valid spawned ui entity");
    }
    if let Some(scope) = &spec.theme_scope {
        world
            .attach_component(&entity, scope.clone())
            .expect("valid spawned ui entity");
    }
    if spec.scroll {
        world
            .attach_component(&entity, UiScrollContainer)
            .expect("valid spawned ui entity");
        world
            .attach_component(&entity, UiScroll::default())
            .expect("valid spawned ui entity");
    }

    for child in &spec.children {
        let child_entity = spawn_node(world, child, Some(entity));
        if let Some(store) = world.get_component_store_mut::<UiNode>()
            && let Some(node) = store.get_mut_store().get_component_mut(entity.index)
        {
            node.children.push(child_entity);
        }
    }

    if let Some(mut forest) = world.get_resource_mut::<UiForest>() {
        forest.insert(spec.id.clone(), entity);
    }
    entity
}

#[derive(Clone, Debug, Default)]
struct NodeState {
    scroll: Option<UiScroll>,
    text: Option<String>,
    interaction: UiInteraction,
}

fn snapshot_state(world: &World, roots: &[Entity]) -> (HashMap<UiId, NodeState>, Option<UiId>) {
    let mut state = HashMap::new();
    for root in roots {
        snapshot_node(world, *root, &mut state);
    }
    let focused = world
        .get_resource::<crate::focus::UiFocusState>()
        .and_then(|focus| focus.focused)
        .and_then(|entity| {
            world.get_component_store::<UiNode>().and_then(|store| {
                store
                    .get_component(entity.index)
                    .map(|node| node.id.clone())
            })
        });
    (state, focused)
}

fn snapshot_node(world: &World, entity: Entity, out: &mut HashMap<UiId, NodeState>) {
    let Some(node) = world
        .get_component_store::<UiNode>()
        .and_then(|store| store.get_component(entity.index).cloned())
    else {
        return;
    };
    let state = NodeState {
        scroll: world
            .get_component_store::<UiScroll>()
            .and_then(|store| store.get_component(entity.index).copied()),
        text: world.get_component_store::<UiText>().and_then(|store| {
            store
                .get_component(entity.index)
                .map(|text| text.value.clone())
        }),
        interaction: world
            .get_component_store::<UiInteraction>()
            .and_then(|store| store.get_component(entity.index).copied())
            .unwrap_or_default(),
    };
    out.insert(node.id.clone(), state);
    for child in &node.children {
        snapshot_node(world, *child, out);
    }
}

fn restore_state(world: &mut World, state: &HashMap<UiId, NodeState>, focused: Option<UiId>) {
    for (id, saved) in state {
        let Some(entity) = world
            .get_resource::<UiForest>()
            .and_then(|forest| forest.find(id))
        else {
            continue;
        };
        if let Some(scroll) = saved.scroll
            && let Some(store) = world.get_component_store_mut::<UiScroll>()
        {
            store.get_mut_store().attach(entity.index, scroll);
        }
        if let Some(text) = &saved.text
            && let Some(store) = world.get_component_store_mut::<UiText>()
        {
            store.get_mut_store().attach(
                entity.index,
                UiText {
                    value: text.clone(),
                },
            );
        }
        if let Some(store) = world.get_component_store_mut::<UiInteraction>() {
            store
                .get_mut_store()
                .attach(entity.index, saved.interaction);
        }
    }
    if let Some(focused) = focused
        && let Some(entity) = world
            .get_resource::<UiForest>()
            .and_then(|forest| forest.find(&focused))
    {
        crate::focus::focus_entity(world, entity);
    }
}

/// Reloads a document from an in-memory source, preserving state keyed by `UiId`.
///
/// Useful for hot-reload tooling and tests. The registry entry must exist.
pub fn reload_document_from_source(
    world: &mut World,
    name: &str,
    source: &str,
) -> Result<bool, String> {
    ensure_registry(world);
    let Some(entry) = world
        .get_resource::<UiDocumentRegistry>()
        .and_then(|registry| registry.get(name).cloned())
    else {
        return Err(format!("unknown UI document '{name}'"));
    };
    let document = UiDocument::from_ron(source)?;
    document.validate()?;

    let (state, focused) = snapshot_state(world, &entry.roots);
    let old_roots = world
        .get_resource_mut::<UiForest>()
        .map(|mut forest| forest.clear_document(name))
        .unwrap_or_default();
    for root in &old_roots {
        despawn_tree(world, *root);
    }

    let roots = spawn_document(world, &document, Some(name))?;
    restore_state(world, &state, focused);
    if let Some(mut registry) = world.get_resource_mut::<UiDocumentRegistry>()
        && let Some(entry) = registry.entries.get_mut(name)
    {
        entry.document = document;
        entry.roots = roots;
    }
    Ok(true)
}

/// Reloads a document when its asset mtime changed. Returns whether it reloaded.
pub fn reload_document(world: &mut World, name: &str) -> Result<bool, String> {
    ensure_registry(world);
    let Some(entry) = world
        .get_resource::<UiDocumentRegistry>()
        .and_then(|registry| registry.get(name).cloned())
    else {
        return Err(format!("unknown UI document '{name}'"));
    };
    let manager = ignit_file_manager::FileManager::global().map_err(|error| error.to_string())?;
    let modified = manager.asset_modified(&entry.path);
    if modified == entry.modified {
        return Ok(false);
    }
    let source = manager
        .read_asset_to_string(&entry.path)
        .map_err(|error| error.to_string())?;
    reload_document_from_source(world, name, &source)?;
    if let Some(mut registry) = world.get_resource_mut::<UiDocumentRegistry>()
        && let Some(entry) = registry.entries.get_mut(name)
    {
        entry.modified = modified;
    }
    Ok(true)
}

/// Reloads every registered document whose asset changed.
pub fn reload_changed_documents(world: &mut World) -> Result<Vec<String>, String> {
    let names: Vec<String> = world
        .get_resource::<UiDocumentRegistry>()
        .map(|registry| registry.entries.keys().cloned().collect())
        .unwrap_or_default();
    let mut reloaded = Vec::new();
    for name in names {
        if reload_document(world, &name)? {
            reloaded.push(name);
        }
    }
    Ok(reloaded)
}

fn despawn_tree(world: &mut World, entity: Entity) {
    let children = world
        .get_component_store::<UiNode>()
        .and_then(|store| {
            store
                .get_component(entity.index)
                .map(|node| node.children.clone())
        })
        .unwrap_or_default();
    for child in children {
        despawn_tree(world, child);
    }
    if let Some(mut forest) = world.get_resource_mut::<UiForest>() {
        forest.remove_entity(entity);
    }
    world.despawn_entity(&entity);
}

fn ensure_forest(world: &mut World) {
    if world.get_resource::<UiForest>().is_none() {
        world.insert_resource(UiForest::default());
    }
}

fn ensure_registry(world: &mut World) {
    if world.get_resource::<UiDocumentRegistry>().is_none() {
        world.insert_resource(UiDocumentRegistry::default());
    }
}
