pub mod align;
pub mod array;
pub mod arrayedit;
pub mod attedit;
pub mod blend;
pub mod block_edit;
pub mod break_cmd;
pub mod bvstate;
pub mod copy;
pub mod delete;
mod entity_index;
pub mod explode;
pub mod fillet;
pub mod flatten;
pub mod geom;
pub mod join;
pub mod lengthen;
pub mod mirror;
pub mod mledit;
pub mod ncopy;
pub mod offset;
pub mod overkill;
pub mod pedit;
pub mod refedit;
pub mod reverse;
pub mod rotate;
pub mod scale;
pub mod setbylayer;
pub mod spline_ops;
pub mod splinedit;
pub mod stretch;
pub mod torient;
pub mod translate;
pub mod trim;
pub mod txtexp;

/// Large interactive transformations must not allocate or transform hundreds
/// of thousands of WireModel ghosts for every pointer movement. Sample
/// spatially spread source wires for a responsive *temporary* geometric
/// preview; the commit always transforms every selected entity.
pub(super) fn mass_preview_stride(wires: usize) -> usize {
    if wires <= 8_192 || std::env::var_os("OCS_FULL_MASS_PREVIEW").is_some() {
        1
    } else {
        wires.div_ceil(2_048)
    }
}

/// The sampled ghost still needs to communicate the full selected extent.
/// A cheap 2D outline preserves that spatial reference while every geometry
/// edit remains exact on commit. This is not a raster or a replacement object.
pub(super) fn mass_preview_extent(
    wires: &[crate::scene::model::wire_model::WireModel],
) -> Option<crate::scene::model::wire_model::WireModel> {
    if mass_preview_stride(wires.len()) == 1 {
        return None;
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for wire in wires {
        let bb = wire.aabb;
        if !bb.iter().all(|v| v.is_finite())
            || bb[0] > bb[2]
            || bb[1] > bb[3]
        {
            continue;
        }
        bounds[0] = bounds[0].min(f64::from(bb[0]));
        bounds[1] = bounds[1].min(f64::from(bb[1]));
        bounds[2] = bounds[2].max(f64::from(bb[2]));
        bounds[3] = bounds[3].max(f64::from(bb[3]));
    }
    if !bounds.iter().all(|v| v.is_finite()) {
        return None;
    }
    let [x0, y0, x1, y1] = bounds;
    Some(crate::scene::model::wire_model::WireModel::solid_f64(
        "__mass_preview_outline__".into(),
        vec![
            [x0, y0, 0.0],
            [x1, y0, 0.0],
            [x1, y1, 0.0],
            [x0, y1, 0.0],
            [x0, y0, 0.0],
        ],
        crate::scene::model::wire_model::WireModel::CYAN,
        false,
    ))
}

/// ARRAY can multiply one selection into thousands of preview copies.
pub(super) fn mass_array_preview_stride(wires: usize, copies: usize) -> usize {
    let total = wires.saturating_mul(copies);
    if total <= 8_192 || std::env::var_os("OCS_FULL_MASS_PREVIEW").is_some() {
        1
    } else {
        wires.div_ceil((2_048 / copies.max(1)).max(1)).max(1)
    }
}

#[cfg(test)]
mod mass_preview_tests {
    use super::{mass_array_preview_stride, mass_preview_stride};

    #[test]
    fn massive_array_preview_budget_accounts_for_all_copies() {
        let stride = mass_array_preview_stride(54_433, 12);
        assert!(stride > 1);
        assert!(54_433usize.div_ceil(stride) * 12 <= 2_048);
        assert_eq!(mass_array_preview_stride(100, 3), 1);
    }

    #[test]
    fn massive_wire_preview_is_bounded_without_affecting_small_edits() {
        assert_eq!(mass_preview_stride(200), 1);
        assert_eq!(mass_preview_stride(8192), 1);
        let stride = mass_preview_stride(327_364);
        assert!(stride > 1);
        assert!(327_364usize.div_ceil(stride) <= 2_048);
        assert_eq!(54_433usize.div_ceil(mass_preview_stride(54_433)) <= 2_048, true);
    }
}
