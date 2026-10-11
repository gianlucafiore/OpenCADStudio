//! Declarative dynamic-block behavior definitions and per-INSERT overrides.
//!
//! This is a non-destructive evaluation layer. It intentionally uses the
//! existing named-parameter evaluator, not a second expression engine or
//! constraint solver. Rendering/persistence adapters are separate work.
use std::collections::HashMap;

use super::named_parameters::{ParamError, ParameterTable};

/// A behavior attached to a named parameter, evaluated relative to its
/// definition-time value. Entity IDs are block-definition member handles.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockAction {
    pub parameter: String,
    pub target_handles: Vec<u64>,
    pub kind: BlockActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockActionKind {
    MoveX,
    MoveY,
    Rotate,
    ScaleUniform,
}

/// An action's value computed without mutating shared block geometry.
/// Rotation uses radians; move uses drawing units; scale is a factor.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedAction {
    pub target_handles: Vec<u64>,
    pub kind: BlockActionKind,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicBlockDefinition {
    pub name: String,
    pub enabled: bool,
    pub parameters: ParameterTable,
    pub actions: Vec<BlockAction>,
}

impl DynamicBlockDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: false,
            parameters: ParameterTable::new(),
            actions: Vec::new(),
        }
    }

    pub fn add_action(&mut self, action: BlockAction) -> Result<(), ParamError> {
        if !self.parameters.contains(&action.parameter) {
            return Err(ParamError::UndefinedReference(action.parameter));
        }
        self.actions.push(action);
        Ok(())
    }

    /// Evaluate one insertion against its definition, without changing any
    /// shared entities or the definition's parameter table.
    pub fn evaluate(
        &self,
        instance: &DynamicBlockInstance,
    ) -> Result<Vec<EvaluatedAction>, ParamError> {
        if !self.enabled {
            return Ok(Vec::new());
        }

        let mut effective = self.parameters.clone();
        for (name, expression) in &instance.overrides {
            if !effective.contains(name) {
                return Err(ParamError::UndefinedReference(name.clone()));
            }
            effective.set(name, expression)?;
        }

        // Resolve both tables, including dependent expressions, so one changed
        // width can drive other named values. No cumulative transforms.
        self.actions
            .iter()
            .map(|action| {
                let base = self.parameters.resolve(&action.parameter)?;
                let current = effective.resolve(&action.parameter)?;
                if !base.is_finite() || !current.is_finite() {
                    return Err(ParamError::Parse(
                        "dynamic block parameter must be finite".into(),
                    ));
                }
                let value = match action.kind {
                    BlockActionKind::MoveX
                    | BlockActionKind::MoveY
                    | BlockActionKind::Rotate => current - base,
                    BlockActionKind::ScaleUniform => {
                        if base.abs() < 1e-12 {
                            return Err(ParamError::DivisionByZero);
                        }
                        current / base
                    }
                };
                Ok(EvaluatedAction {
                    target_handles: action.target_handles.clone(),
                    kind: action.kind,
                    value,
                })
            })
            .collect()
    }
}

/// Identified by the INSERT's persistent handle; different INSERTs keep
/// different values even though they refer to the same block definition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicBlockInstance {
    pub insert_handle: u64,
    pub overrides: HashMap<String, String>,
}

impl DynamicBlockInstance {
    pub fn new(insert_handle: u64) -> Self {
        Self { insert_handle, overrides: HashMap::new() }
    }

    pub fn set_override(
        &mut self,
        definition: &DynamicBlockDefinition,
        name: &str,
        expression: &str,
    ) -> Result<(), ParamError> {
        if !definition.parameters.contains(name) {
            return Err(ParamError::UndefinedReference(name.to_owned()));
        }
        let mut candidate = definition.parameters.clone();
        for (key, value) in &self.overrides {
            candidate.set(key, value)?;
        }
        candidate.set(name, expression)?;
        // Explicitly validate the edited value even for a disabled definition
        // or a parameter not yet linked to an action.
        let resolved = candidate.resolve(name)?;
        if !resolved.is_finite() {
            return Err(ParamError::Parse("parameter must be finite".into()));
        }
        // Validate values of all actions now, so invalid updates aren't stored.
        let mut updated = self.clone();
        updated.overrides.insert(name.to_owned(), expression.to_owned());
        definition.evaluate(&updated)?;
        self.overrides = updated.overrides;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DynamicBlockDefinition {
        let mut definition = DynamicBlockDefinition::new("DOOR");
        definition.enabled = true;
        definition.parameters.set("width", "0.9").unwrap();
        definition.parameters.set("frame", "width + 0.1").unwrap();
        definition.add_action(BlockAction {
            parameter: "frame".into(),
            target_handles: vec![10, 11],
            kind: BlockActionKind::MoveX,
        }).unwrap();
        definition
    }

    #[test]
    fn independent_insert_overrides_do_not_change_definition() {
        let definition = sample();
        let mut first = DynamicBlockInstance::new(100);
        let second = DynamicBlockInstance::new(101);
        first.set_override(&definition, "width", "1.2").unwrap();
        let moved = definition.evaluate(&first).unwrap();
        assert!((moved[0].value - 0.3).abs() < 1e-9);
        assert_eq!(moved[0].target_handles, vec![10, 11]);
        assert!(definition.evaluate(&second).unwrap()[0].value.abs() < 1e-9);
        assert_eq!(definition.parameters.resolve("width").unwrap(), 0.9);
    }

    #[test]
    fn invalid_override_does_not_mutate_instance() {
        let definition = sample();
        let mut first = DynamicBlockInstance::new(1);
        assert!(first.set_override(&definition, "width", "frame + 1").is_err());
        assert!(first.overrides.is_empty());
        assert!(first.set_override(&definition, "missing", "5").is_err());
    }

    #[test]
    fn disabled_definition_does_not_generate_actions() {
        let mut definition = sample();
        definition.enabled = false;
        assert!(definition.evaluate(&DynamicBlockInstance::new(1)).unwrap().is_empty());
    }

    #[test]
    fn scale_uses_ratio_not_additive_delta() {
        let mut definition = DynamicBlockDefinition::new("TABLE");
        definition.enabled = true;
        definition.parameters.set("size", "2").unwrap();
        definition.add_action(BlockAction {
            parameter: "size".into(),
            target_handles: vec![2],
            kind: BlockActionKind::ScaleUniform,
        }).unwrap();
        let mut instance = DynamicBlockInstance::new(3);
        instance.set_override(&definition, "size", "3").unwrap();
        assert_eq!(definition.evaluate(&instance).unwrap()[0].value, 1.5);
    }
}
