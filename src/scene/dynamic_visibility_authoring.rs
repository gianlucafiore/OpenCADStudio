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

#[cfg(test)]
mod tests {
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
