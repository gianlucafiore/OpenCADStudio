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

#[cfg(test)]
mod mass_preview_tests {
    use super::mass_preview_stride;

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
