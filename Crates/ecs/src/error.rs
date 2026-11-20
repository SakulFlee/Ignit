use std::{any::TypeId, error::Error, fmt::Display};

use crate::Entity;

#[derive(Debug)]
pub enum ECSError {
    InvalidEntity(Entity),
    ComponentStoreNotExisting,
    /// The entity does not currently hold the component, so it cannot be
    /// replaced. Use `World::insert_component_any` to attach it first.
    ComponentNotAttached(Entity),
    /// A type-erased component value did not match the type held by the store.
    ComponentTypeMismatch(TypeId),
}

impl Display for ECSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ECSError::InvalidEntity(entity) => {
                write!(
                    f,
                    "Invalid Entity (Index: {}, Generation: {}). Index invalid or generation was superseded!",
                    entity.index, entity.generation
                )
            }
            ECSError::ComponentStoreNotExisting => {
                write!(f, "There is no ComponentStore for the requested Component!")
            }
            ECSError::ComponentNotAttached(entity) => {
                write!(
                    f,
                    "Entity (Index: {}, Generation: {}) does not hold the requested Component!",
                    entity.index, entity.generation
                )
            }
            ECSError::ComponentTypeMismatch(type_id) => {
                write!(
                    f,
                    "The provided value is not of the stored Component type ({type_id:?})!"
                )
            }
        }
    }
}

impl Error for ECSError {}

#[cfg(test)]
mod tests {
    use crate::{ECSError, Entity};

    #[test]
    fn test_error_display() {
        let error = ECSError::InvalidEntity(Entity::new(1, 123));
        let output = error.to_string();
        assert!(output.contains("Invalid Entity"));
    }

    #[test]
    fn display_prints_one_message_without_the_debug_representation() {
        let error = ECSError::InvalidEntity(Entity::new(1, 123));

        assert_eq!(
            error.to_string(),
            "Invalid Entity (Index: 1, Generation: 123). Index invalid or generation was superseded!"
        );
    }

    #[test]
    fn display_covers_every_variant() {
        assert_eq!(
            ECSError::ComponentStoreNotExisting.to_string(),
            "There is no ComponentStore for the requested Component!"
        );
        assert_eq!(
            ECSError::ComponentNotAttached(Entity::new(7, 3)).to_string(),
            "Entity (Index: 7, Generation: 3) does not hold the requested Component!"
        );

        let type_id = std::any::TypeId::of::<u32>();
        assert_eq!(
            ECSError::ComponentTypeMismatch(type_id).to_string(),
            format!("The provided value is not of the stored Component type ({type_id:?})!")
        );
    }
}
