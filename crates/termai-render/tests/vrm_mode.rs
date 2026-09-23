//! RP-08 at the operation level: the wrap mode has an owner, and switching it is a
//! display-layer operation that never reaches the grid (kernel/03 section 3.8 / K-11).

use termai_core::grid::{
    Cell, CursorState, Damage, GridDelta, GridSnapshot, RowPayload, LINE_WRAPPED,
};
use termai_render::mirror::{DamageSet, Mirror};
use termai_render::vrm::{VrmState, WrapMode};

/// A mirror holding a 3-row grid whose rows 0 and 1 both continue into the next row, i.e.
/// exactly one 3-row logical line, built only by applying real `GridDelta`s that carry
/// `LINE_WRAPPED` (never by hand-building a snapshot).
fn wrapped_mirror() -> Mirror {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(GridSnapshot::new(4, 3));
    for (rev, row) in [(1_u32, 0_u16), (2, 1)] {
        let mut cells = vec![Cell::BLANK; 4];
        cells[0].ch = char::from(b'a' + row as u8);
        let delta = GridDelta {
            rev,
            scroll: None,
            damage: Damage::default(),
            rows: vec![RowPayload {
                row,
                cells,
                flags: LINE_WRAPPED,
            }],
            cursor: CursorState::default(),
        };
        mirror.apply_delta(&delta).expect("the delta applies");
    }
    mirror
}

#[test]
fn switching_wrap_mode_emits_no_grid_delta() {
    let mut mirror = wrapped_mirror();
    let mut vrm = VrmState::new();
    let rev_before = mirror.rev();
    let bytes_before = mirror.grid().expect("a grid").canonical_bytes();

    vrm.set_mode(WrapMode::Fold);
    vrm.set_mode(WrapMode::Clip);
    vrm.set_mode(WrapMode::Fold);

    assert_eq!(
        vrm.mode(),
        WrapMode::Fold,
        "the switch is a pure mode change"
    );
    assert_eq!(mirror.rev(), rev_before, "switching must not bump the rev");
    assert_eq!(
        mirror.grid().expect("a grid").canonical_bytes(),
        bytes_before,
        "switching must not change one canonical grid byte"
    );
    assert_eq!(
        mirror.take_damage(),
        DamageSet::default(),
        "switching must not queue damage"
    );
}

#[test]
fn visual_rows_length_follows_the_mode() {
    // A three-row logical line (a chain) with the cursor outside it.
    let mut snapshot = GridSnapshot::new(4, 3);
    snapshot.row_flags[0] = LINE_WRAPPED;
    snapshot.row_flags[1] = LINE_WRAPPED;

    let mut vrm = VrmState::new();
    assert_eq!(
        vrm.mode(),
        WrapMode::Fold,
        "AR-22 section 4 default is Fold"
    );

    let fold = vrm.visual_rows(&snapshot, 3);
    assert_eq!(fold.len(), 3);
    assert_eq!(
        fold.iter().map(|v| v.segment).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(fold.iter().all(|v| !v.clipped));

    vrm.set_mode(WrapMode::Clip);
    let clip = vrm.visual_rows(&snapshot, 3);
    assert_eq!(clip.len(), 1);
    assert_eq!(clip[0].segment, 0, "cursor outside the line shows the head");
    assert!(clip[0].clipped, "the tail of the chain is hidden");
}

#[test]
fn grid_delta_driven_projection_follows_the_mode() {
    let mirror = wrapped_mirror();
    let grid = mirror.grid().expect("a grid");
    let mut vrm = VrmState::new();

    // The chain reaches the VRM only through the mirror, never through a hand-built
    // snapshot.
    let fold = vrm.visual_rows(grid, 0);
    assert_eq!(fold.len(), 3);
    assert_eq!(fold[0].logical_start_row, 0);
    assert_eq!(
        fold.iter().map(|v| v.logical_start_row).collect::<Vec<_>>(),
        vec![0, 0, 0]
    );

    vrm.set_mode(WrapMode::Clip);
    let clip = vrm.visual_rows(grid, 0);
    assert_eq!(clip.len(), 1);
    assert_eq!(clip[0].segment, 0);
    assert!(clip[0].clipped);
}
