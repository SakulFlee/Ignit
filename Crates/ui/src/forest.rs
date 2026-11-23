//! Multi-root UI forest and stable ID index.

use std::collections::HashMap;

use ignit_ecs::Entity;

use crate::id::UiId;

/// Resource holding every UI root and the stable-ID index.
#[derive(Debug, Default)]
pub struct UiForest {
    roots: Vec<Entity>,
    by_id: HashMap<UiId, Entity>,
    documents: HashMap<String, Vec<Entity>>,
    revision: u64,
}

impl UiForest {
    pub fn roots(&self) -> &[Entity] {
        &self.roots
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn bump(&mut self) {
        self.revision += 1;
    }

    pub fn find(&self, id: &UiId) -> Option<Entity> {
        self.by_id.get(id).copied()
    }

    pub fn entity_id(&self, entity: Entity) -> Option<&UiId> {
        self.by_id
            .iter()
            .find_map(|(id, candidate)| (*candidate == entity).then_some(id))
    }

    pub fn insert(&mut self, id: UiId, entity: Entity) {
        self.by_id.insert(id, entity);
        self.revision += 1;
    }

    pub fn remove(&mut self, id: &UiId) -> Option<Entity> {
        let entity = self.by_id.remove(id);
        if entity.is_some() {
            self.revision += 1;
        }
        entity
    }

    pub fn remove_entity(&mut self, entity: Entity) {
        self.by_id.retain(|_, candidate| *candidate != entity);
        self.roots.retain(|candidate| *candidate != entity);
        for roots in self.documents.values_mut() {
            roots.retain(|candidate| *candidate != entity);
        }
        self.revision += 1;
    }

    pub fn add_root(&mut self, entity: Entity, document: Option<&str>) {
        if !self.roots.contains(&entity) {
            self.roots.push(entity);
        }
        if let Some(document) = document {
            let roots = self.documents.entry(document.to_string()).or_default();
            if !roots.contains(&entity) {
                roots.push(entity);
            }
        }
        self.revision += 1;
    }

    pub fn remove_root(&mut self, entity: Entity) {
        self.roots.retain(|candidate| *candidate != entity);
        for roots in self.documents.values_mut() {
            roots.retain(|candidate| *candidate != entity);
        }
        self.revision += 1;
    }

    pub fn document_roots(&self, document: &str) -> &[Entity] {
        self.documents
            .get(document)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn documents(&self) -> impl Iterator<Item = (&str, &[Entity])> {
        self.documents
            .iter()
            .map(|(name, roots)| (name.as_str(), roots.as_slice()))
    }

    pub fn clear_document(&mut self, document: &str) -> Vec<Entity> {
        let roots = self.documents.remove(document).unwrap_or_default();
        for root in &roots {
            self.roots.retain(|candidate| candidate != root);
        }
        self.revision += 1;
        roots
    }

    pub fn index(&self) -> UiIndex<'_> {
        UiIndex { forest: self }
    }
}

/// Read-only stable-ID lookup view.
#[derive(Clone, Copy, Debug)]
pub struct UiIndex<'a> {
    forest: &'a UiForest,
}

impl UiIndex<'_> {
    pub fn get(&self, id: &UiId) -> Option<Entity> {
        self.forest.find(id)
    }

    pub fn contains(&self, id: &UiId) -> bool {
        self.forest.find(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.forest.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forest.by_id.is_empty()
    }
}
