//! Authoring helpers for native DWG block visibility parameters.
//!
//! Operates directly on the opencadcodec visibility model that imported
//! AutoCAD blocks already use. Does not create a parallel action engine.
//! Linking the resulting parameter into the block evaluation graph and the
//! DWG writer is a separate integration step.
use std::collections::HashSet;

use codec::objects::{BlockVisibilityParameter, BlockVisibilityState};
use codec::Handle;

/// Errors which should be displayed to the BEDIT author before modifying a
/// shared block definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityAuthorError {
    NoStates,
    EmptyName,
    DuplicateName(String),
    UnknownMember(u64),
    DuplicateMember(u64),
    UnknownState(String),
}

impl std::fmt::Display for VisibilityAuthorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VisibilityAuthorError {}

/// Build the codec's *native* parameter data from authored choices.
///
/// Membership is validated against actual block-definition members, never
/// against model-space INSERT handles. This avoids broken state mappings.
pub fn build_visibility_parameter(
    name: &str,
    members: &[Handle],
    states: Vec<(String, Vec<Handle>)>,
    grip_point: codec::types::Vector3,
) -> Result<BlockVisibilityParameter, VisibilityAuthorError> {
    if states.is_empty() {
        return Err(VisibilityAuthorError::NoStates);
    }
    if name.trim().is_empty() {
        return Err(VisibilityAuthorError::EmptyName);
    }
    let mut member_set = HashSet::new();
    for handle in members {
        if !member_set.insert(handle.value()) {
            return Err(VisibilityAuthorError::DuplicateMember(handle.value()));
        }
    }
    let mut names = HashSet::new();
    let mut output = Vec::with_capacity(states.len());
    for (state_name, visible) in states {
        let trimmed = state_name.trim();
        if trimmed.is_empty() {
            return Err(VisibilityAuthorError::EmptyName);
        }
        if !names.insert(trimmed.to_lowercase()) {
            return Err(VisibilityAuthorError::DuplicateName(trimmed.to_string()));
        }
        let mut seen = HashSet::new();
        let mut visible_blocks = Vec::new();
        for handle in visible {
            if !member_set.contains(&handle.value()) {
                return Err(VisibilityAuthorError::UnknownMember(handle.value()));
            }
            if seen.insert(handle.value()) {
                visible_blocks.push(handle);
            }
        }
        output.push(BlockVisibilityState {
            name: trimmed.to_owned(),
            visible_blocks,
            visible_params: Vec::new(),
        });
    }
    Ok(BlockVisibilityParameter {
        name: name.trim().to_owned(),
        element_name: name.trim().to_owned(),
        def_point: grip_point,
        all_blocks: members.to_vec(),
        states: output,
        show_properties: true,
        is_initialized: true,
        ..Default::default()
    })
}

/// Rename one authored state, retaining its member set and insertion order.
/// Names are unique case-insensitively just as during initial authoring.
pub fn rename_visibility_state(
    parameter: &mut BlockVisibilityParameter,
    old_name: &str,
    new_name: &str,
) -> Result<(), VisibilityAuthorError> {
    let renamed = new_name.trim();
    if renamed.is_empty() {
        return Err(VisibilityAuthorError::EmptyName);
    }
    let Some(index) = parameter.states.iter().position(|s| s.name == old_name) else {
        return Err(VisibilityAuthorError::UnknownState(old_name.into()));
    };
    if parameter.states.iter().enumerate().any(|(i, s)| {
        i != index && s.name.eq_ignore_ascii_case(renamed)
    }) {
        return Err(VisibilityAuthorError::DuplicateName(renamed.into()));
    }
    parameter.states[index].name = renamed.into();
    Ok(())
}

/// Replace one state's visible members. Validation runs *before* mutation.
/// Repeated selected handles are deduplicated, preserving their first order.
pub fn set_visibility_state_members(
    parameter: &mut BlockVisibilityParameter,
    state_name: &str,
    selected: &[Handle],
) -> Result<(), VisibilityAuthorError> {
    let Some(index) = parameter.states.iter().position(|s| s.name == state_name) else {
        return Err(VisibilityAuthorError::UnknownState(state_name.into()));
    };
    let allowed: HashSet<u64> = parameter.all_blocks.iter().map(|h| h.value()).collect();
    let mut unique = HashSet::new();
    let mut members = Vec::new();
    for handle in selected {
        if !allowed.contains(&handle.value()) {
            return Err(VisibilityAuthorError::UnknownMember(handle.value()));
        }
        if unique.insert(handle.value()) {
            members.push(*handle);
        }
    }
    parameter.states[index].visible_blocks = members;
    Ok(())
}

/// Adds a state without changing which members belong to other states.
pub fn add_visibility_state(
    parameter: &mut BlockVisibilityParameter,
    name: &str,
    selected: &[Handle],
) -> Result<(), VisibilityAuthorError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(VisibilityAuthorError::EmptyName);
    }
    if parameter.states.iter().any(|s| s.name.eq_ignore_ascii_case(trimmed)) {
        return Err(VisibilityAuthorError::DuplicateName(trimmed.into()));
    }
    let mut draft = parameter.clone();
    draft.states.push(BlockVisibilityState {name: trimmed.into(), ..Default::default()});
    set_visibility_state_members(&mut draft, trimmed, selected)?;
    *parameter = draft;
    Ok(())
}

/// The last state cannot be deleted: otherwise the parameter has no choice.
pub fn remove_visibility_state(
    parameter: &mut BlockVisibilityParameter,
    name: &str,
) -> Result<(), VisibilityAuthorError> {
    let Some(index) = parameter.states.iter().position(|s| s.name == name) else {
        return Err(VisibilityAuthorError::UnknownState(name.into()));
    };
    if parameter.states.len() == 1 {
        return Err(VisibilityAuthorError::NoStates);
    }
    parameter.states.remove(index);
    Ok(())
}

/// In-progress BEDIT visibility authoring, separate from the live DWG data.
/// A draft does not modify the drawing until it is validated and compiled.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibilityAuthorDraft {
    pub parameter_name: String,
    pub grip_point: codec::types::Vector3,
    pub member_handles: Vec<Handle>,
    pub states: Vec<(String, Vec<Handle>)>,
}

impl VisibilityAuthorDraft {
    pub fn new(name: impl Into<String>, members: Vec<Handle>) -> Self {
        Self {
            parameter_name: name.into(),
            grip_point: codec::types::Vector3::new(0.0, 0.0, 0.0),
            member_handles: members,
            states: Vec::new(),
        }
    }

    /// Record the current BEDIT selection as the geometry for a new state.
    /// Existing states are never mutated on a failed operation.
    pub fn capture_selection(
        &mut self,
        state_name: &str,
        selected: &[Handle],
    ) -> Result<(), VisibilityAuthorError> {
        let mut trial = self.clone();
        trial.states.push((state_name.to_owned(), selected.to_vec()));
        trial.build()?;
        *self = trial;
        Ok(())
    }

    /// Validate the complete draft and construct codec-native data.
    /// Authoring the evaluation graph and saving to DWG happen elsewhere.
    pub fn build(&self) -> Result<BlockVisibilityParameter, VisibilityAuthorError> {
        build_visibility_parameter(
            &self.parameter_name,
            &self.member_handles,
            self.states.clone(),
            self.grip_point,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bedit_draft_captures_selection_without_editing_live_data() {
        let a = Handle::new(10);
        let b = Handle::new(20);
        let mut draft = VisibilityAuthorDraft::new("View", vec![a, b]);
        draft.capture_selection("Plan", &[a]).unwrap();
        let before = draft.clone();
        assert_eq!(draft.capture_selection("Elevation", &[Handle::new(99)]),
            Err(VisibilityAuthorError::UnknownMember(99)));
        assert_eq!(draft, before);
        draft.capture_selection("Elevation", &[b]).unwrap();
        let p = draft.build().unwrap();
        assert_eq!(p.states[0].visible_blocks, vec![a]);
        assert_eq!(p.states[1].visible_blocks, vec![b]);
    }


    #[test]
    fn authoring_edits_are_validated_and_atomic() {
        let a = Handle::new(10);
        let b = Handle::new(20);
        let mut p = build_visibility_parameter("View", &[a,b], vec![
            ("Plan".into(), vec![a]),
            ("Elevation".into(), vec![b]),
        ], codec::types::Vector3::new(0.0,0.0,0.0)).unwrap();
        let original = p.clone();
        assert_eq!(set_visibility_state_members(&mut p, "Plan", &[Handle::new(999)]),
            Err(VisibilityAuthorError::UnknownMember(999)));
        assert_eq!(p, original);
        assert_eq!(rename_visibility_state(&mut p, "Plan", "elevation"),
            Err(VisibilityAuthorError::DuplicateName("elevation".into())));
        assert_eq!(p, original);
        set_visibility_state_members(&mut p, "Plan", &[b, a, b]).unwrap();
        assert_eq!(p.states[0].visible_blocks, vec![b,a]);
        rename_visibility_state(&mut p, "Plan", "Floor plan").unwrap();
        assert_eq!(p.states[0].name, "Floor plan");
        add_visibility_state(&mut p, "Section", &[a]).unwrap();
        assert_eq!(p.states.len(), 3);
        remove_visibility_state(&mut p, "Section").unwrap();
        remove_visibility_state(&mut p, "Elevation").unwrap();
        assert_eq!(remove_visibility_state(&mut p, "Floor plan"), Err(VisibilityAuthorError::NoStates));
    }


    use super::*;

    #[test]
    fn creates_native_parameter_for_plan_elevation_and_section() {
        let all = [Handle::new(10), Handle::new(20), Handle::new(30)];
        let p = build_visibility_parameter("View", &all, vec![
            ("Plan".into(), vec![all[0]]),
            ("Elevation".into(), vec![all[1]]),
            ("Section".into(), vec![all[2]]),
        ], codec::types::Vector3::new(0.0, 0.0, 0.0)).unwrap();
        assert_eq!(p.states.len(), 3);
        assert_eq!(p.all_blocks, all);
        assert_eq!(p.states[1].visible_blocks, vec![all[1]]);
    }

    #[test]
    fn rejects_bad_state_names_and_foreign_handles() {
        let all = [Handle::new(10)];
        let make = |states| build_visibility_parameter(
            "View", &all, states, codec::types::Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(make(vec![]).unwrap_err(), VisibilityAuthorError::NoStates);
        assert_eq!(make(vec![(" ".into(), vec![])]).unwrap_err(), VisibilityAuthorError::EmptyName);
        assert_eq!(make(vec![("Plan".into(), vec![]), ("plan".into(), vec![])]).unwrap_err(),
            VisibilityAuthorError::DuplicateName("plan".into()));
        assert_eq!(make(vec![("Plan".into(), vec![Handle::new(11)])]).unwrap_err(),
            VisibilityAuthorError::UnknownMember(11));
    }
}
