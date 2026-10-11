//! Declarative dynamic-block behavior definitions and per-INSERT overrides.
//!
//! This is a non-destructive evaluation layer. It intentionally uses the
//! existing named-parameter evaluator, not a second expression engine or
//! constraint solver. Rendering/persistence adapters are separate work.
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use super::named_parameters::{ParamError, ParameterTable};
use codec::types::{Transform, Vector3};
use codec::EntityType;

/// Evaluate an INSERT's member geometry from fresh definition clones.
///
/// This never touches shared CadDocument entities. Callers can render or
/// materialize the resulting block-local geometry separately; the DWG adapter
/// must preserve proper native dynamic-block objects before saving.
pub fn evaluate_member_geometry(
    definition: &DynamicBlockDefinition,
    instance: &DynamicBlockInstance,
    members: &[(u64, EntityType)],
) -> Result<Vec<(u64, EntityType)>, ParamError> {
    let actions = definition.evaluate(instance)?;
    let mut result = members.to_vec();
    for action in actions {
        if !action.value.is_finite() {
            return Err(ParamError::Parse("nonfinite action transform".into()));
        }
        let transform = match action.kind {
            BlockActionKind::MoveX => Transform::from_translation(Vector3::new(action.value, 0.0, 0.0)),
            BlockActionKind::MoveY => Transform::from_translation(Vector3::new(0.0, action.value, 0.0)),
            BlockActionKind::Rotate => Transform::from_rotation(Vector3::UNIT_Z, action.value),
            BlockActionKind::ScaleUniform => Transform::from_scaling_with_origin(
                Vector3::new(action.value, action.value, action.value), Vector3::new(0.0, 0.0, 0.0),
            ),
        };
        for (handle, member) in &mut result {
            if action.target_handles.contains(handle) {
                codec::Entity::apply_transform(member, &transform);
            }
        }
    }
    Ok(result)
}


/// A behavior attached to a named parameter, evaluated relative to its
/// definition-time value. Entity IDs are block-definition member handles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockAction {
    pub parameter: String,
    pub target_handles: Vec<u64>,
    pub kind: BlockActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A versioned, portable snapshot of dynamic behavior, independent of the
/// binary DWG representation. NOT a substitute for native AutoCAD objects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct RegistrySnapshot {
    schema_version: u32,
    definitions: Vec<DefinitionSnapshot>,
    instances: Vec<InstanceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct DefinitionSnapshot {
    name: String,
    enabled: bool,
    parameters: Vec<(String, String)>,
    actions: Vec<BlockAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct InstanceSnapshot {
    insert_handle: u64,
    definition_name: String,
    overrides: Vec<(String, String)>,
}

/// Per-document runtime state for definitions and individual INSERTs.
/// Persistent serialization and native DWG mapping are separate adapters.
#[derive(Debug, Clone, Default)]
pub struct DynamicBlockRegistry {
    definitions: HashMap<String, DynamicBlockDefinition>,
    instances: HashMap<u64, (String, DynamicBlockInstance)>,
}

impl DynamicBlockRegistry {
    /// Serialize to a versioned JSON sidecar for tests and future document IO.
    /// This is not an AutoCAD dynamic-block DWG encoder.
    pub fn to_json(&self) -> Result<String, String> {
        let mut definitions: Vec<_> = self.definitions.values().map(|d| DefinitionSnapshot {
            name: d.name.clone(),
            enabled: d.enabled,
            parameters: d.parameters.iter().map(|p| (p.name.clone(), p.source.clone())).collect(),
            actions: d.actions.clone(),
        }).collect();
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        let mut instances: Vec<_> = self.instances.iter().map(|(&handle, (name, instance))| {
            let mut overrides: Vec<_> = instance.overrides.iter().map(|(k,v)| (k.clone(),v.clone())).collect();
            overrides.sort_by(|a,b| a.0.cmp(&b.0));
            InstanceSnapshot {insert_handle: handle, definition_name: name.clone(), overrides}
        }).collect();
        instances.sort_by_key(|i| i.insert_handle);
        serde_json::to_string_pretty(&RegistrySnapshot {schema_version: 1, definitions, instances})
            .map_err(|e| e.to_string())
    }

    /// Load atomically: validation errors never leak a partially loaded registry.
    pub fn from_json(src: &str) -> Result<Self, String> {
        let snapshot: RegistrySnapshot = serde_json::from_str(src).map_err(|e| e.to_string())?;
        if snapshot.schema_version != 1 {
            return Err(format!("unsupported dynamic block schema {}", snapshot.schema_version));
        }
        let mut registry = Self::default();
        for definition in snapshot.definitions {
            if definition.name.trim().is_empty() || registry.definitions.contains_key(&definition.name) {
                return Err("empty or duplicate dynamic block name".into());
            }
            let mut rebuilt = DynamicBlockDefinition::new(definition.name);
            rebuilt.enabled = definition.enabled;
            for (name, source) in definition.parameters {
                rebuilt.parameters.set(&name, &source).map_err(|e| e.to_string())?;
            }
            for action in definition.actions {
                rebuilt.add_action(action).map_err(|e| e.to_string())?;
            }
            registry.register_definition(rebuilt);
        }
        for instance in snapshot.instances {
            if registry.instances.contains_key(&instance.insert_handle) {
                return Err("duplicate INSERT identity".into());
            }
            registry.attach_insert(instance.insert_handle, &instance.definition_name)
                .map_err(|e| e.to_string())?;
            for (name, expression) in instance.overrides {
                registry.set_insert_parameter(instance.insert_handle, &name, &expression)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(registry)
    }

    pub fn register_definition(&mut self, definition: DynamicBlockDefinition) {
        self.definitions.insert(definition.name.clone(), definition);
    }

    pub fn definition(&self, name: &str) -> Option<&DynamicBlockDefinition> {
        self.definitions.get(name)
    }

    /// Attach an INSERT to an existing dynamic definition. The same INSERT
    /// can be attached only once; reattaching clears its old overrides.
    pub fn attach_insert(&mut self, insert_handle: u64, definition_name: &str) -> Result<(), ParamError> {
        if insert_handle == 0 {
            return Err(ParamError::Parse("INSERT handle must not be null".into()));
        }
        if !self.definitions.contains_key(definition_name) {
            return Err(ParamError::UndefinedReference(definition_name.to_owned()));
        }
        self.instances.insert(insert_handle, (
            definition_name.to_owned(), DynamicBlockInstance::new(insert_handle),
        ));
        Ok(())
    }

    pub fn detach_insert(&mut self, insert_handle: u64) {
        self.instances.remove(&insert_handle);
    }

    pub fn set_insert_parameter(&mut self, insert_handle: u64, name: &str, expression: &str) -> Result<(), ParamError> {
        let (definition_name, instance) = self.instances.get_mut(&insert_handle)
            .ok_or_else(|| ParamError::UndefinedReference(format!("INSERT {insert_handle}")))?;
        let definition = self.definitions.get(definition_name)
            .ok_or_else(|| ParamError::UndefinedReference(definition_name.clone()))?;
        instance.set_override(definition, name, expression)
    }

    pub fn evaluate_insert(
        &self,
        insert_handle: u64,
        members: &[(u64, EntityType)],
    ) -> Result<Vec<(u64, EntityType)>, ParamError> {
        let (definition_name, instance) = self.instances.get(&insert_handle)
            .ok_or_else(|| ParamError::UndefinedReference(format!("INSERT {insert_handle}")))?;
        let definition = self.definitions.get(definition_name)
            .ok_or_else(|| ParamError::UndefinedReference(definition_name.clone()))?;
        evaluate_member_geometry(definition, instance, members)
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
    #[test]
    fn geometry_is_independent_for_two_inserts() {
        let definition = sample();
        let source = codec::entities::Line {
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(1.0, 0.0, 0.0),
            ..Default::default()
        };
        let members = vec![(10, EntityType::Line(source.clone())), (12, EntityType::Line(source.clone()))];
        let mut wide = DynamicBlockInstance::new(100);
        wide.set_override(&definition, "width", "1.2").unwrap();
        let narrow = DynamicBlockInstance::new(101);
        let first = evaluate_member_geometry(&definition, &wide, &members).unwrap();
        let second = evaluate_member_geometry(&definition, &narrow, &members).unwrap();
        let EntityType::Line(ref moved) = first[0].1 else { panic!("expected line") };
        let EntityType::Line(ref untouched) = second[0].1 else { panic!("expected line") };
        assert!((moved.start.x - 0.3).abs() < 1e-9);
        assert_eq!(untouched.start.x, 0.0);
        let EntityType::Line(ref other) = first[1].1 else { panic!("expected line") };
        assert_eq!(other.start.x, 0.0);
        let EntityType::Line(ref original) = members[0].1 else { panic!("expected line") };
        assert_eq!(original.start.x, 0.0);
    }

    #[test]
    fn json_roundtrip_preserves_independent_insert_values() {
        let mut registry = DynamicBlockRegistry::default();
        registry.register_definition(sample());
        registry.attach_insert(123, "DOOR").unwrap();
        registry.attach_insert(124, "DOOR").unwrap();
        registry.set_insert_parameter(123, "width", "1.4").unwrap();
        let json = registry.to_json().unwrap();
        let restored = DynamicBlockRegistry::from_json(&json).unwrap();
        assert_eq!(json, restored.to_json().unwrap());
        let source = vec![(10, EntityType::Line(codec::entities::Line {
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(1.0, 0.0, 0.0),
            ..Default::default()
        }))];
        let (EntityType::Line(wide), EntityType::Line(normal)) = (
            &restored.evaluate_insert(123, &source).unwrap()[0].1,
            &restored.evaluate_insert(124, &source).unwrap()[0].1,
        ) else { panic!("line expected") };
        assert!((wide.start.x - 0.5).abs() < 1e-9);
        assert_eq!(normal.start.x, 0.0);
    }

    #[test]
    fn json_rejects_unknown_version() {
        let registry = DynamicBlockRegistry::default();
        let json = registry.to_json().unwrap().replace("schema_version", "schema_version_invalid");
        assert!(DynamicBlockRegistry::from_json(&json).is_err());
    }

    #[test]
    fn registry_tracks_insert_values_independently() {
        let mut registry = DynamicBlockRegistry::default();
        registry.register_definition(sample());
        registry.attach_insert(100, "DOOR").unwrap();
        registry.attach_insert(101, "DOOR").unwrap();
        registry.set_insert_parameter(100, "width", "1.2").unwrap();
        let source = vec![(10, EntityType::Line(codec::entities::Line {
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(1.0, 0.0, 0.0),
            ..Default::default()
        }))];
        let wider = registry.evaluate_insert(100, &source).unwrap();
        let original = registry.evaluate_insert(101, &source).unwrap();
        let (EntityType::Line(w), EntityType::Line(n)) = (&wider[0].1, &original[0].1) else { panic!("line expected") };
        assert!((w.start.x - 0.3).abs() < 1e-9);
        assert_eq!(n.start.x, 0.0);
        registry.detach_insert(100);
        assert!(registry.evaluate_insert(100, &source).is_err());
        assert!(registry.evaluate_insert(101, &source).is_ok());
    }

    #[test]
    fn repeated_evaluation_does_not_accumulate_transforms() {
        let definition = sample();
        let members = vec![(10, EntityType::Line(codec::entities::Line {
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(1.0, 0.0, 0.0),
            ..Default::default()
        }))];
        let mut instance = DynamicBlockInstance::new(99);
        instance.set_override(&definition, "width", "1.2").unwrap();
        let once = evaluate_member_geometry(&definition, &instance, &members).unwrap();
        let twice = evaluate_member_geometry(&definition, &instance, &members).unwrap();
        let (EntityType::Line(a), EntityType::Line(b)) = (&once[0].1, &twice[0].1) else { panic!("expected lines") };
        assert_eq!(a.start.x, b.start.x);
    }

}
