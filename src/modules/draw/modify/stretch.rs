// Stretch tool — ribbon definition + interactive command.
//
// Command:  STRETCH (SS)
//   Workflow:
//     1. Pick first corner of the crossing window (right-to-left = crossing).
//     2. Pick second corner.
//     3. Pick base point.
//     4. Pick new point → stretches only vertices inside the crossing window.
//
//   Entity behaviour:
//     Line        : move start if inside, move end if inside, move both if both inside.
//     LwPolyline  : move each vertex independently.
//     Polyline/P2D: move each vertex independently.
//     Arc         : ends inside move; a lone end keeps the arc's height.
//     Circle      : move the whole entity if its center is inside the window.
//     Insert      : move the whole entity if its insertion point is inside.
//     Spline      : control and fit points inside move.
//     All others  : left as they are.

use crate::t;
use codec::Handle;
use glam::DVec3;

use crate::command::{CadCommand, CmdResult};
use crate::modules::{IconKind, ModuleEvent, ToolDef};
use crate::scene::model::wire_model::WireModel;

// ── Stretching one entity ──────────────────────────────────────────────────

/// An ordered vertex-selection region. Shift means remove from the current
/// vertex set; a later ordinary crossing window can add the same point back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StretchWindow {
    pub min: DVec3,
    pub max: DVec3,
    pub subtract: bool,
}

impl StretchWindow {
    pub fn add(min: DVec3, max: DVec3) -> Self {
        Self { min, max, subtract: false }
    }

    pub fn contains_xy(&self, x: f64, y: f64) -> bool {
        x >= self.min.x && x <= self.max.x && y >= self.min.y && y <= self.max.y
    }
}

/// Evaluate the windows in pick order, like the selection marquee: the
/// latest window covering a vertex decides whether it moves.
pub fn vertex_selected(windows: &[StretchWindow], x: f64, y: f64) -> bool {
    windows.iter().rev()
        .find(|window| window.contains_xy(x, y))
        .is_some_and(|window| !window.subtract)
}


/// Stretch `entity` by `delta`: the points of it that lie in one of the
/// crossing `windows` (world XY) move, the rest stay. Arcs and polyline arc
/// segments keep their height (sagitta) as the chord under them changes.
/// Returns whether anything moved. The command's preview and its commit
/// both go through here.
pub fn stretch_entity(
    entity: &mut codec::EntityType,
    windows: &[StretchWindow],
    delta: DVec3,
) -> bool {
    let in_win = |x: f64, y: f64| -> bool { vertex_selected(windows, x, y) };
    let (dx, dy, dz) = (delta.x, delta.y, delta.z);
    let mut stretched = false;
    match entity {
        codec::EntityType::Line(l) => {
            let s_in = in_win(l.start.x, l.start.y);
            let e_in = in_win(l.end.x, l.end.y);
            if s_in {
                l.start.x += dx;
                l.start.y += dy;
                l.start.z += dz;
                stretched = true;
            }
            if e_in {
                l.end.x += dx;
                l.end.y += dy;
                l.end.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::LwPolyline(p) => {
            let Some(mut world) = crate::entities::curve::lwpolyline_world_xy(p) else {
                return false;
            };
            let before: Vec<[f64; 2]> =
                world.vertices.iter().map(|v| [v.location.x, v.location.y]).collect();
            for v in &mut world.vertices {
                if in_win(v.location.x, v.location.y) {
                    v.location.x += dx;
                    v.location.y += dy;
                    stretched = true;
                }
            }
            if stretched {
                let after: Vec<[f64; 2]> =
                    world.vertices.iter().map(|v| [v.location.x, v.location.y]).collect();
                let closed = world.is_closed;
                for (k, v) in world.vertices.iter_mut().enumerate() {
                    v.bulge = held_bulge(v.bulge, &before, &after, k, closed);
                }
                *p = world;
            }
        }
        codec::EntityType::Polyline2D(p) => {
            let before: Vec<[f64; 2]> =
                p.vertices.iter().map(|v| [v.location.x, v.location.y]).collect();
            for v in &mut p.vertices {
                if in_win(v.location.x, v.location.y) {
                    v.location.x += dx;
                    v.location.y += dy;
                    stretched = true;
                }
            }
            if stretched {
                let after: Vec<[f64; 2]> =
                    p.vertices.iter().map(|v| [v.location.x, v.location.y]).collect();
                let closed = p.is_closed();
                for (k, v) in p.vertices.iter_mut().enumerate() {
                    v.bulge = held_bulge(v.bulge, &before, &after, k, closed);
                }
            }
        }
        codec::EntityType::Polyline(p) => {
            for v in &mut p.vertices {
                if in_win(v.location.x, v.location.y) {
                    v.location.x += dx;
                    v.location.y += dy;
                    stretched = true;
                }
            }
        }
        codec::EntityType::Arc(a) => stretched = stretch_arc(a, &in_win, delta),
        codec::EntityType::Circle(c) => {
            if in_win(c.center.x, c.center.y) {
                c.center.x += dx;
                c.center.y += dy;
                c.center.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::Ellipse(e) => {
            if in_win(e.center.x, e.center.y) {
                e.center.x += dx;
                e.center.y += dy;
                e.center.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::Insert(ins) => {
            if in_win(ins.insert_point.x, ins.insert_point.y) {
                ins.insert_point.x += dx;
                ins.insert_point.y += dy;
                ins.insert_point.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::Text(t) => {
            if in_win(t.insertion_point.x, t.insertion_point.y) {
                t.insertion_point.x += dx;
                t.insertion_point.y += dy;
                t.insertion_point.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::MText(t) => {
            if in_win(t.insertion_point.x, t.insertion_point.y) {
                t.insertion_point.x += dx;
                t.insertion_point.y += dy;
                t.insertion_point.z += dz;
                stretched = true;
            }
        }
        codec::EntityType::Viewport(vp) => {
            // Viewport STRETCH edits its corners. Evaluate the final
            // additive/subtractive region set once per original corner;
            // checking the centre would miss partial corner selections.
            stretched = crate::entities::viewport::stretch_where(vp, &in_win, delta);
        }
        codec::EntityType::Dimension(dim) => {
            use codec::entities::Dimension;
            // Move every definition point that falls inside the
            // window — the same points the grips expose — then
            // refresh the stored measurement so the value tracks
            // the stretched geometry.
            let mv = |p: &mut codec::types::Vector3| {
                if in_win(p.x, p.y) {
                    p.x += dx;
                    p.y += dy;
                    p.z += dz;
                    true
                } else {
                    false
                }
            };
            match &mut **dim {
                Dimension::Linear(d) => {
                    stretched |= mv(&mut d.first_point);
                    stretched |= mv(&mut d.second_point);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Aligned(d) => {
                    stretched |= mv(&mut d.first_point);
                    stretched |= mv(&mut d.second_point);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Radius(d) => {
                    stretched |= mv(&mut d.angle_vertex);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Diameter(d) => {
                    stretched |= mv(&mut d.angle_vertex);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Angular2Ln(d) => {
                    stretched |= mv(&mut d.angle_vertex);
                    stretched |= mv(&mut d.first_point);
                    stretched |= mv(&mut d.second_point);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Angular3Pt(d) => {
                    stretched |= mv(&mut d.angle_vertex);
                    stretched |= mv(&mut d.first_point);
                    stretched |= mv(&mut d.second_point);
                    stretched |= mv(&mut d.definition_point);
                }
                Dimension::Ordinate(d) => {
                    stretched |= mv(&mut d.feature_location);
                    stretched |= mv(&mut d.leader_endpoint);
                }
                Dimension::Arc(d) => {
                    stretched |= mv(&mut d.definition_point);
                    stretched |= mv(&mut d.first_extension_point);
                    stretched |= mv(&mut d.second_extension_point);
                    stretched |= mv(&mut d.center_point);
                    if d.has_leader {
                        stretched |= mv(&mut d.first_leader_point);
                        stretched |= mv(&mut d.second_leader_point);
                    }
                }
                Dimension::LargeRadial(d) => {
                    stretched |= mv(&mut d.definition_point);
                    stretched |= mv(&mut d.chord_point);
                    stretched |= mv(&mut d.override_center);
                    stretched |= mv(&mut d.jog_point);
                }
            }
            // Pinned text follows too; the zero sentinel means
            // "auto placement" and must not be captured by a
            // window that happens to cover the origin.
            let t = dim.base().text_middle_point;
            if t.x * t.x + t.y * t.y + t.z * t.z > 1e-16 {
                let mut t = t;
                if mv(&mut t) {
                    dim.base_mut().text_middle_point = t;
                    stretched = true;
                }
            }
            if stretched {
                dim.base_mut().actual_measurement = dim.measurement();
            }
        }
        codec::EntityType::Polyline3D(p) => {
            for v in &mut p.vertices {
                if in_win(v.position.x, v.position.y) {
                    v.position.x += dx;
                    v.position.y += dy;
                    v.position.z += dz;
                    stretched = true;
                }
            }
        }
        // The control and fit points in the window move; the rest stay.
        codec::EntityType::Spline(s) => {
            for p in s.control_points.iter_mut().chain(s.fit_points.iter_mut()) {
                if in_win(p.x, p.y) {
                    p.x += dx;
                    p.y += dy;
                    p.z += dz;
                    stretched = true;
                }
            }
        }
        codec::EntityType::Point(p) => {
            if in_win(p.location.x, p.location.y) {
                p.location.x += dx;
                p.location.y += dy;
                p.location.z += dz;
                stretched = true;
            }
        }
        _ => {}
    }
    stretched
}

/// The bulge of the segment from vertex `k` once its ends moved from
/// `before` to `after`: the arc keeps its height, bulge · chord / 2.
pub(crate) fn held_bulge(
    bulge: f64,
    before: &[[f64; 2]],
    after: &[[f64; 2]],
    k: usize,
    closed: bool,
) -> f64 {
    let next = if k + 1 < before.len() {
        k + 1
    } else if closed {
        0
    } else {
        return bulge;
    };
    let chord = |p: &[[f64; 2]]| (p[next][0] - p[k][0]).hypot(p[next][1] - p[k][1]);
    let (old, new) = (chord(before), chord(after));
    if bulge == 0.0 || old <= 1e-12 || new <= 1e-12 {
        bulge
    } else {
        bulge * old / new
    }
}

/// Stretch an arc by its ends: both in the window move it whole; one moves
/// that end while the arc keeps its height over the new chord.
fn stretch_arc(arc: &mut codec::entities::Arc, in_win: &dyn Fn(f64, f64) -> bool, delta: DVec3) -> bool {
    use codec::types::{Matrix3, Vector3};
    let ocs = Matrix3::arbitrary_axis(arc.normal);
    let end = |angle: f64| {
        Vector3::new(
            arc.center.x + arc.radius * angle.cos(),
            arc.center.y + arc.radius * angle.sin(),
            arc.center.z,
        )
    };
    let (start, finish) = (end(arc.start_angle), end(arc.end_angle));
    let (start_w, finish_w) = (ocs * start, ocs * finish);
    let (start_in, finish_in) = (in_win(start_w.x, start_w.y), in_win(finish_w.x, finish_w.y));
    if !start_in && !finish_in {
        return false;
    }
    let local = ocs.transpose() * Vector3::new(delta.x, delta.y, delta.z);
    if start_in && finish_in {
        arc.center = arc.center + local;
        return true;
    }
    let sweep = (arc.end_angle - arc.start_angle).rem_euclid(std::f64::consts::TAU);
    let shift = |p: Vector3, moved: bool| if moved { p + local } else { p };
    let (a, b) = (shift(start, start_in), shift(finish, finish_in));
    let old_chord = (finish.x - start.x).hypot(finish.y - start.y);
    let new_chord = (b.x - a.x).hypot(b.y - a.y);
    if old_chord <= 1e-12 || new_chord <= 1e-12 {
        return false;
    }
    let bulge = (sweep / 4.0).tan() * old_chord / new_chord;
    // The arc through `a` and `b` with that bulge, counter-clockwise from
    // `a`: its centre sits off the chord's midpoint by the apothem.
    let sweep = 4.0 * bulge.atan();
    let radius = new_chord / (2.0 * (sweep / 2.0).sin());
    let apothem = radius * (sweep / 2.0).cos();
    let (ux, uy) = ((b.x - a.x) / new_chord, (b.y - a.y) / new_chord);
    let centre = Vector3::new(
        (a.x + b.x) / 2.0 - uy * apothem,
        (a.y + b.y) / 2.0 + ux * apothem,
        arc.center.z,
    );
    arc.center = centre;
    arc.radius = radius.abs();
    arc.start_angle = (a.y - centre.y).atan2(a.x - centre.x);
    arc.end_angle = (b.y - centre.y).atan2(b.x - centre.x);
    true
}

// ── Ribbon definition ──────────────────────────────────────────────────────

pub fn tool() -> ToolDef {
    ToolDef {
        id: "STRETCH",
        label: "Stretch",
        icon: IconKind::Svg(include_bytes!("../../../../assets/icons/stretch.svg")),
        event: ModuleEvent::Command("STRETCH".to_string()),
    }
}

// ── Command implementation ─────────────────────────────────────────────────

enum Step {
    /// Waiting for the first corner of another crossing window.
    WindowCorner1,
    /// Waiting for the opposite corner.
    WindowCorner2(DVec3),
    /// Selection is complete; waiting for the displacement base point.
    Base,
    /// Waiting for the displacement target.
    Target { base: DVec3 },
}

pub struct StretchCommand {
    handles: Vec<Handle>,
    wire_models: Vec<WireModel>,
    /// Independent crossing windows accumulated during the selection stage.
    windows: Vec<StretchWindow>,
    step: Step,
    shift: bool,
}

impl StretchCommand {
    pub fn new(handles: Vec<Handle>, wire_models: Vec<WireModel>) -> Self {
        Self {
            handles,
            wire_models,
            windows: Vec::new(),
            step: Step::WindowCorner1,
            shift: false,
        }
    }

    /// A selection made by a crossing window before the command: that window
    /// marks what moves, so the command goes straight to the base point.
    pub fn preselected(
        handles: Vec<Handle>,
        wire_models: Vec<WireModel>,
        window: (DVec3, DVec3),
    ) -> Self {
        Self {
            handles,
            wire_models,
            windows: vec![StretchWindow::add(window.0, window.1)],
            step: Step::Base,
            shift: false,
        }
    }

    /// Continue gathering crossing windows after the host has resolved the
    /// entities touched by the latest window.
    pub fn with_windows(
        handles: Vec<Handle>,
        wire_models: Vec<WireModel>,
        windows: Vec<StretchWindow>,
    ) -> Self {
        Self {
            handles,
            wire_models,
            windows,
            step: Step::WindowCorner1,
            shift: false,
        }
    }
    /// A window enclosing every vertex of the current selection, or `None`
    /// when nothing is selected. Padded so a vertex exactly on the boundary
    /// counts as inside rather than depending on the comparison's edge.
    fn selection_bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        let mut any = false;
        for wire in &self.wire_models {
            for index in 0..wire.points.len() {
                let point = wire_point(wire, index);
                if !point.is_finite() {
                    continue;
                }
                min = min.min(point);
                max = max.max(point);
                any = true;
            }
        }
        if !any {
            return None;
        }
        let pad = (max - min).length().max(1.0) * 1e-6;
        Some((min - DVec3::splat(pad), max + DVec3::splat(pad)))
    }
}

/// A wire vertex rebuilt from its double-single halves.
fn wire_point(wire: &WireModel, index: usize) -> DVec3 {
    let high = wire.points[index];
    let low = wire.points_low.get(index).copied().unwrap_or([0.0; 3]);
    DVec3::new(
        high[0] as f64 + low[0] as f64,
        high[1] as f64 + low[1] as f64,
        high[2] as f64 + low[2] as f64,
    )
}

impl CadCommand for StretchCommand {
    fn name(&self) -> &'static str {
        "STRETCH"
    }

    fn set_shift(&mut self, shift: bool) {
        self.shift = shift;
    }

    fn prompt(&self) -> String {
        match &self.step {
            Step::WindowCorner1 => {
                if self.windows.is_empty() && self.handles.is_empty() {
                    t!("STRETCH  Specify first corner of crossing window:").into_owned()
                } else {
                    t!(
                        "STRETCH  Specify first corner of another crossing window, or press Enter to continue:"
                    )
                    .into_owned()
                }
            }
            Step::WindowCorner2(_) => t!("STRETCH  Specify opposite corner:").into_owned(),
            Step::Base => t!("STRETCH  Specify base point:").into_owned(),
            Step::Target { base } => {
                let bx = format!("{:.3}", base.x);
                let bz = format!("{:.3}", base.z);
                t!(
                    "STRETCH  Specify new point  [base %{bx},%{bz}]:",
                    bx = bx,
                    bz = bz
                )
                .into_owned()
            }
        }
    }

    fn on_point(&mut self, pt: DVec3) -> CmdResult {
        match &self.step {
            Step::WindowCorner1 => {
                self.step = Step::WindowCorner2(pt);
                CmdResult::NeedPoint
            }

            Step::WindowCorner2(c1) => {
                let win_min = c1.min(pt);
                let win_max = c1.max(pt);

                let mut windows = self.windows.clone();
                windows.push(StretchWindow { min: win_min, max: win_max, subtract: self.shift });

                // Hand the accumulated selection back to the host. The host resolves
                // the entities touched by this window and relaunches STRETCH still in
                // the selection stage, so another crossing window can be drawn.
                CmdResult::StretchWindow {
                    handles: self.handles.clone(),
                    windows,
                }
            }

            Step::Base => {
                self.step = Step::Target { base: pt };
                CmdResult::NeedPoint
            }

            Step::Target { base } => {
                let delta = pt - *base;

                CmdResult::StretchEntities {
                    handles: self.handles.clone(),
                    windows: self.windows.clone(),
                    delta,
                }
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        if let Step::WindowCorner1 = self.step {
            // Preserve the existing preselection behaviour: if STRETCH started
            // from a selected set and no explicit crossing window was drawn,
            // treat its complete bounds as the stretch window.
            if self.windows.is_empty() {
                if let Some((win_min, win_max)) = self.selection_bounds() {
                    self.windows.push(StretchWindow::add(win_min, win_max));
                }
            }

            if !self.handles.is_empty() && !self.windows.is_empty() {
                self.step = Step::Base;
                return CmdResult::NeedPoint;
            }
        }

        CmdResult::Cancel
    }
    fn on_escape(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn window_corner_pick(&self) -> bool {
        // The two crossing-window corners are free points; Ortho/Polar must not
        // pin the opposite corner to an axis or the window becomes a line (#291).
        matches!(self.step, Step::WindowCorner1 | Step::WindowCorner2(_))
    }

    fn selects_by_window(&self) -> bool {
        // Crossing windows select what stretches; dynamic input has nothing
        // to enter there (#657).
        self.window_corner_pick()
    }

    fn window_first_corner(&self) -> Option<DVec3> {
        // Expose the first corner so the host draws a filled crossing marquee to
        // the cursor, matching a normal box selection instead of a bare outline.
        match &self.step {
            Step::WindowCorner2(c1) => Some(*c1),
            _ => None,
        }
    }

    fn preview_keep_resident(&self, document: &codec::CadDocument) -> bool {
        if !matches!(self.step, Step::Target { .. }) {
            return false;
        }
        // Cloning/tessellating all selected INSERTs on *each mouse move* can
        // recursively expand nested block stubs into many thousands of wires.
        // Large plain selections, polylines and splines can be expensive too.
        // Leave resident originals visible and use the cheap displacement
        // rubber-band instead. The committed stretch remains unchanged.
        const MAX_FULL_PREVIEW_ENTITIES: usize = 128;
        const MAX_FULL_PREVIEW_VERTICES: usize = 1024;
        if self.handles.len() > MAX_FULL_PREVIEW_ENTITIES {
            return true;
        }
        let mut vertices = 0usize;
        for handle in &self.handles {
            match document.get_entity(*handle) {
                Some(codec::EntityType::Insert(_)) => return true,
                Some(codec::EntityType::LwPolyline(p)) => vertices = vertices.saturating_add(p.vertices.len()),
                Some(codec::EntityType::Polyline2D(p)) => vertices = vertices.saturating_add(p.vertices.len()),
                Some(codec::EntityType::Polyline3D(p)) => vertices = vertices.saturating_add(p.vertices.len()),
                Some(codec::EntityType::Polyline(p)) => vertices = vertices.saturating_add(p.vertices.len()),
                Some(codec::EntityType::Spline(p)) => {
                    vertices = vertices.saturating_add(p.control_points.len()).saturating_add(p.fit_points.len());
                }
                _ => {}
            }
            if vertices > MAX_FULL_PREVIEW_VERTICES {
                return true;
            }
        }
        false
    }

    fn preview_lightweight_wires(&self, pt: DVec3) -> Vec<WireModel> {
        let Step::Target { base } = &self.step else {
            return Vec::new();
        };
        let delta = pt - *base;
        // At most 768 simplified vertices per motion event, regardless of
        // how many entities or how finely tessellated a polyline is. Unlike
        // preview_entities this never copies a document entity, tessellates a
        // curve, or expands a nested block.
        const MAX_VERTICES: usize = 768;
        const MAX_WIRES: usize = 48;
        let mut remaining = MAX_VERTICES;
        let mut out = Vec::new();
        for wire in &self.wire_models {
            if remaining < 2 || out.len() >= MAX_WIRES {
                break;
            }
            // Instances and nested stubs are not raw world-space polylines.
            // A misleading per-vertex deformation would be worse than the
            // displacement guide; their committed transform is unchanged.
            if wire.render_instance.is_some() || wire.stub.is_some() || wire.points.len() < 2 {
                continue;
            }
            let count = wire.points.len();
            let sample_count = count.min(remaining.min(128).max(2));
            let mut points = Vec::with_capacity(sample_count);
            // Choose inclusive endpoints. A simple stride truncated at the
            // sample cap could omit the final vertex, precisely the endpoint
            // whose deformation the user most needs to see.
            for k in 0..sample_count {
                let index = k * (count - 1) / (sample_count - 1);
                let mut point = wire_point(wire, index);
                if vertex_selected(&self.windows, point.x, point.y) {
                    point += delta;
                }
                points.push([point.x, point.y, point.z]);
            }
            if points.len() >= 2 {
                remaining -= points.len();
                let mut ghost = WireModel::solid_f64(
                    "__stretch_lightweight__".into(),
                    points,
                    [0.24, 0.76, 0.95, 0.8],
                    false,
                );
                ghost.set_fixed_screen_width(1.2);
                out.push(ghost);
            }
        }
        out
    }

    fn preview_entities(&mut self, document: &codec::CadDocument, pt: DVec3) -> Vec<codec::EntityType> {
        let Step::Target { base } = &self.step else {
            return Vec::new();
        };
        if self.preview_keep_resident(document) {
            return Vec::new();
        }
        let delta = pt - *base;
        self.handles
            .iter()
            .filter_map(|handle| document.get_entity(*handle))
            .map(|entity| {
                let mut entity = entity.clone();
                stretch_entity(&mut entity, &self.windows, delta);
                entity
            })
            .collect()
    }

    fn preview_hidden_handles(&self) -> &[Handle] {
        match self.step {
            Step::Target { .. } => &self.handles,
            _ => &[],
        }
    }

    fn on_preview_wires(&mut self, pt: DVec3) -> Vec<WireModel> {
        match &self.step {
            // The crossing-window rectangle is drawn as a filled selection
            // marquee by the host (via window_first_corner) so it matches a
            // normal box selection — nothing to draw here. (#291)
            Step::WindowCorner2(_) => vec![],
            // The stretched entities themselves come from `preview_entities`.
            Step::Target { base } => {
                vec![WireModel::solid(
                    "rubber_band".into(),
                    vec![
                        [base.x as f32, base.y as f32, base.z as f32],
                        [pt.x as f32, pt.y as f32, pt.z as f32],
                    ],
                    WireModel::CYAN,
                    false,
                )]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod shift_stretch_tests {
    use super::*;
    use codec::entities::Line;
    use codec::types::Vector3;
    use codec::EntityType;

    fn region(x0: f64, y0: f64, x1: f64, y1: f64, subtract: bool) -> StretchWindow {
        StretchWindow {
            min: DVec3::new(x0, y0, -1.0),
            max: DVec3::new(x1, y1, 1.0),
            subtract,
        }
    }

    fn line() -> EntityType {
        let mut line = Line::new();
        line.start = Vector3::new(0.0, 0.0, 0.0);
        line.end = Vector3::new(10.0, 0.0, 0.0);
        EntityType::Line(line)
    }

    #[test]
    fn shift_window_removes_only_the_covered_vertex() {
        let windows = [
            region(-1.0, -1.0, 11.0, 1.0, false),
            region(-1.0, -1.0, 1.0, 1.0, true),
        ];
        assert!(!vertex_selected(&windows, 0.0, 0.0));
        assert!(vertex_selected(&windows, 10.0, 0.0));
        let mut entity = line();
        assert!(stretch_entity(
            &mut entity,
            &windows,
            DVec3::new(0.0, 5.0, 0.0),
        ));
        let EntityType::Line(line) = entity else {
            panic!("line expected");
        };
        assert_eq!(line.start.y, 0.0);
        assert_eq!(line.end.y, 5.0);
    }

    #[test]
    fn later_additive_window_reselects_a_shift_removed_vertex() {
        let windows = [
            region(-1.0, -1.0, 11.0, 1.0, false),
            region(-1.0, -1.0, 1.0, 1.0, true),
            region(-1.0, -1.0, 1.0, 1.0, false),
        ];
        let mut entity = line();
        assert!(vertex_selected(&windows, 0.0, 0.0));
        assert!(stretch_entity(
            &mut entity,
            &windows,
            DVec3::new(0.0, 5.0, 0.0),
        ));
        let EntityType::Line(line) = entity else {
            panic!("line expected");
        };
        assert_eq!(line.start.y, 5.0);
        assert_eq!(line.end.y, 5.0);
    }

    #[test]
    fn shift_excludes_one_polyline_vertex_without_dropping_neighbours() {
        use codec::entities::{LwPolyline, LwVertex};
        use codec::types::Vector2;

        let mut polyline = LwPolyline::new();
        polyline.vertices = [0.0, 10.0, 20.0]
            .into_iter()
            .map(|x| LwVertex::new(Vector2::new(x, 0.0)))
            .collect();
        let windows = [
            region(-1.0, -1.0, 21.0, 1.0, false),
            region(9.0, -1.0, 11.0, 1.0, true),
        ];
        let mut entity = EntityType::LwPolyline(polyline);
        assert!(stretch_entity(
            &mut entity,
            &windows,
            DVec3::new(0.0, 3.0, 0.0),
        ));
        let EntityType::LwPolyline(after) = entity else {
            panic!("expected LW polyline");
        };
        assert_eq!(after.vertices[0].location.y, 3.0);
        assert_eq!(after.vertices[1].location.y, 0.0);
        assert_eq!(after.vertices[2].location.y, 3.0);
    }

    #[test]
    fn shifted_opposite_corner_records_a_subtractive_selection() {
        let mut cmd = StretchCommand::new(Vec::new(), Vec::new());
        assert!(matches!(cmd.on_point(DVec3::new(-1.0, -1.0, 0.0)), CmdResult::NeedPoint));
        cmd.set_shift(true);
        let result = cmd.on_point(DVec3::new(1.0, 1.0, 0.0));
        let CmdResult::StretchWindow { windows, .. } = result else {
            panic!("STRETCH must request another selection window");
        };
        assert_eq!(windows.len(), 1);
        assert!(windows[0].subtract);
    }

    #[test]
    fn large_or_block_stretch_keeps_originals_during_live_preview() {
        let mut scene = crate::scene::Scene::new();
        scene.define_block_raw("LARGE_BLOCK", Vector3::ZERO, vec![line()]);
        let mut document = scene.document;
        let a = document.add_entity(line()).expect("line");
        let mut dense = StretchCommand::with_windows(
            vec![a; 129],
            vec![WireModel::solid_f64(
                "selected_line".into(),
                vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
                WireModel::CYAN,
                false,
            )],
            vec![region(-1.0, -1.0, 1.0, 1.0, false)],
        );
        dense.step = Step::Target { base: DVec3::ZERO };
        assert!(dense.preview_keep_resident(&document));
        assert!(dense.preview_entities(&document, DVec3::new(0.0, 5.0, 0.0)).is_empty());
        assert_eq!(dense.on_preview_wires(DVec3::new(0.0, 5.0, 0.0)).len(), 1);
        let ghost = dense.preview_lightweight_wires(DVec3::new(0.0, 5.0, 0.0));
        assert_eq!(ghost.len(), 1, "heavy preview should show a sampled deformed outline");
        assert_eq!(wire_point(&ghost[0], 0), DVec3::new(0.0, 5.0, 0.0));
        assert_eq!(wire_point(&ghost[0], 1), DVec3::new(10.0, 0.0, 0.0));

        let b = document.add_entity(EntityType::Insert(codec::entities::Insert::new(
            "LARGE_BLOCK", Vector3::ZERO,
        ))).expect("insert");
        let mut block = StretchCommand::with_windows(
            vec![b], vec![], vec![region(-1.0, -1.0, 1.0, 1.0, false)],
        );
        block.step = Step::Target { base: DVec3::ZERO };
        assert!(block.preview_keep_resident(&document));
        assert!(block.preview_entities(&document, DVec3::new(3.0, 0.0, 0.0)).is_empty());
        // The original geometry is never modified by the preview.
        let EntityType::Insert(ref ins) = document.get_entity(b).unwrap() else { panic!("insert") };
        assert_eq!(ins.insert_point, Vector3::ZERO);

        let mut small = StretchCommand::with_windows(
            vec![a], vec![], vec![region(-1.0, -1.0, 11.0, 1.0, false)],
        );
        small.step = Step::Target { base: DVec3::ZERO };
        assert!(!small.preview_keep_resident(&document));
        assert_eq!(small.preview_entities(&document, DVec3::new(0.0, 5.0, 0.0)).len(), 1);
    }

    #[test]
    fn stretch_preview_replaces_hidden_original_with_modified_geometry() {
        let mut document = codec::CadDocument::new();
        let handle = document.add_entity(line()).expect("add line");
        let windows = vec![
            region(-1.0, -1.0, 11.0, 1.0, false),
            region(-1.0, -1.0, 1.0, 1.0, true),
        ];
        let mut cmd = StretchCommand::with_windows(vec![handle], Vec::new(), windows);
        cmd.step = Step::Target { base: DVec3::ZERO };
        assert_eq!(cmd.preview_hidden_handles(), &[handle]);
        let preview = cmd.preview_entities(&document, DVec3::new(0.0, 5.0, 0.0));
        assert_eq!(preview.len(), 1);
        let EntityType::Line(ref stretched) = preview[0] else {
            panic!("expected line preview");
        };
        assert_eq!(stretched.start.y, 0.0);
        assert_eq!(stretched.end.y, 5.0);
        let EntityType::Line(original) = document.get_entity(handle).expect("original") else {
            panic!("expected original line");
        };
        assert_eq!(original.end.y, 0.0, "preview must not edit the document");
    }
}
