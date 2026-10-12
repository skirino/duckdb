//! The child-selection arithmetic of `VectorArrayBuffer`.
//!
//! An array vector is a fixed number of child rows per row, so reading a slice
//! of one means expanding the row selection into a child selection: child row
//! `i * array_size + j` comes from array row `sel[offset + i]`, element `j`.
//! `Verify` additionally compacts, because a NULL array row contributes
//! nothing.
//!
//! The C++ spelled this out three times over, in `VerifyInternal`,
//! `FlattenSliceInternal` and `CopyInternal`. All three are the same formula
//! apart from whether a NULL array row contributes its child rows, so it lives
//! here once, with the validity mask deciding that.

use crate::vector::selection::{
    Selection,
    Validity,
    sel_t,
};

/// Expands `count` array rows into the child selection vector that gathers
/// their elements: `out[i * array_size + j] == sel[offset + i] * array_size +
/// j`.
///
/// The rows start at `offset`, which is the source row of the copy, so the
/// selection is read at `offset + row` — matching the C++ loop the array
/// buffers spelled out.
///
/// This is the selection `VectorArrayBuffer::FlattenSliceInternal` and
/// `VectorArrayBuffer::CopyInternal` hand to the child's flatten/copy. Those
/// pass an absent validity mask: a NULL array row still owns its child rows, so
/// every row contributes, which is what leaves the child row-aligned with the
/// parent. `VectorArrayBuffer::VerifyInternal` passes the real mask, and then a
/// NULL row contributes nothing and the selection is dense over the valid rows.
pub fn array_child_sel(
    selection: &[u32],
    validity: &[u64],
    offset: usize,
    count: usize,
    array_size: usize,
) -> Result<Vec<u32>, String> {
    if array_size == 0
    {
        return Err("array_child_sel: array size must be non-zero".to_string());
    }
    let selection = Selection::new(selection);
    let validity = Validity::new(validity);
    let child_count = count
        .checked_mul(array_size)
        .ok_or_else(|| "array_child_sel: child row count overflows".to_string())?;
    let mut out = Vec::with_capacity(child_count);
    for row in 0..count
    {
        let mapped = selection.index(offset + row)?;
        if !validity.row_is_valid(mapped)?
        {
            continue;
        }
        for element in 0..array_size
        {
            out.push(sel_t(mapped * array_size + element)?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_a_slice_of_arrays() {
        let selection = [2u32, 0u32];
        assert_eq!(array_child_sel(&selection, &[], 0, 2, 3), Ok(vec![6, 7, 8, 0, 1, 2]));
    }

    #[test]
    fn expands_an_identity_slice_of_arrays() {
        assert_eq!(array_child_sel(&[], &[], 0, 3, 2), Ok(vec![0, 1, 2, 3, 4, 5]));
    }

    #[test]
    fn rejects_an_empty_array() {
        assert!(array_child_sel(&[], &[], 0, 1, 0).is_err());
    }

    #[test]
    fn compacts_null_array_rows_out_of_the_child_selection() {
        // rows 0 and 2 valid, row 1 NULL
        let validity = [0b101u64];
        let selection = [2u32, 1u32, 0u32];
        assert_eq!(array_child_sel(&selection, &validity, 0, 3, 2), Ok(vec![4, 5, 0, 1]));
    }

    #[test]
    fn keeps_null_array_rows_when_the_mask_is_absent() {
        // the flatten and copy paths have no mask to compact against
        let selection = [2u32, 1u32, 0u32];
        assert_eq!(array_child_sel(&selection, &[], 0, 3, 2), Ok(vec![4, 5, 2, 3, 0, 1]));
    }

    // A copy that starts part-way into the selection reads the selection at
    // `offset + row`, not at `row`: `CopyInternal` reads its source rows from
    // `source_offset` onward.
    #[test]
    fn the_offset_indexes_the_selection() {
        let selection = [2u32, 5u32, 1u32, 4u32];
        // rows 2 and 3 of the selection, so array rows 1 and 4
        assert_eq!(array_child_sel(&selection, &[], 2, 2, 2), Ok(vec![2, 3, 8, 9]));
    }

    #[test]
    fn rejects_a_selection_that_is_too_short() {
        assert!(array_child_sel(&[1u32], &[], 0, 2, 1).is_err());
    }
}
