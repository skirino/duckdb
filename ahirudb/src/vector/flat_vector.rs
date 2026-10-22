//! The byte-copy kernels of `FlatVector` and `StandardVectorBuffer`.
//!
//! These are the `memcpy` loops the C++ module specializes by type size. They
//! are the hottest pointer arithmetic in the module — every flatten and every
//! copy of a fixed-width column goes through them — and they take nothing but
//! raw buffers and a selection vector, so they cross the Crubit boundary as
//! slices.
//!
//! The C++ side used four functions (`FixedFlattenCopy`, `FlattenVectorBuffer`,
//! `FixedSizeCopy`, `CopyVectorBuffer`) because the call shapes differed; they
//! are the same loop, so Rust exposes it once, with the two base offsets
//! spelled out.

use crate::vector::selection::Selection;

/// Copies `copy_count` entries of `type_size` bytes from `source` to `target`.
///
/// Output row `target_offset + i` is filled from source row
/// `source_sel[source_offset + i]`. `source_sel` is empty for the identity
/// selection.
///
/// The target buffer is often larger than the range being written — a flat
/// vector buffer holds at least `STANDARD_VECTOR_SIZE` entries — so
/// `copy_count` is passed rather than derived, and the buffers are only
/// required to *cover* the named range.
pub fn copy_vector_buffer(
    target: &mut [u8],
    source: &[u8],
    source_sel: &[u32],
    source_offset: usize,
    target_offset: usize,
    copy_count: usize,
    type_size: usize,
) -> Result<(), String> {
    if type_size == 0 {
        return Err("copy_vector_buffer: type size must be non-zero".to_string());
    }
    let target_bytes = copy_count
        .checked_mul(type_size)
        .ok_or_else(|| "copy_vector_buffer: copy count overflows the target buffer".to_string())?;
    let target_start = target_offset.checked_mul(type_size).ok_or_else(|| {
        "copy_vector_buffer: target offset overflows the target buffer".to_string()
    })?;
    let target_end = target_start.checked_add(target_bytes).ok_or_else(|| {
        "copy_vector_buffer: target range overflows the target buffer".to_string()
    })?;
    if target_end > target.len() {
        return Err(format!(
            "copy_vector_buffer: target buffer holds {} bytes but the copy needs {target_end}",
            target.len()
        ));
    }

    let selection = Selection::new(source_sel);
    let target = &mut target[target_start..target_end];
    // The widths the C++ `switch` specialized its copy on, now one
    // monomorphized body each, so the compiler sees a constant stride. `0`
    // asks for the runtime `type_size` instead, which is what the C++ left
    // to its `default:` arm; nothing calls this through the boundary with
    // one of those often enough for the extra body to matter.
    match type_size {
        1 => copy_rows::<1>(target, source, selection, source_offset, type_size),
        2 => copy_rows::<2>(target, source, selection, source_offset, type_size),
        4 => copy_rows::<4>(target, source, selection, source_offset, type_size),
        8 => copy_rows::<8>(target, source, selection, source_offset, type_size),
        16 => copy_rows::<16>(target, source, selection, source_offset, type_size),
        _ => copy_rows::<0>(target, source, selection, source_offset, type_size),
    }
}

/// The copy itself, one body per type size.
///
/// `N` is the compile-time type size the C++ `switch` specialized its copy on;
/// `0` asks for the runtime `type_size`, for the widths the C++ left to its
/// `default:` arm. A `&mut [u8]` carries no stride, so this is the only place
/// the compiler can learn it — and it has to be a constant to be worth having,
/// because the only call to this function comes through the Crubit boundary,
/// where nothing would have specialized it.
///
/// `target` has already been sliced to exactly the range being written, so
/// `chunks_exact_mut` yields one chunk per row and the loop bounds itself.
fn copy_rows<const N: usize>(
    target: &mut [u8],
    source: &[u8],
    selection: Selection,
    source_offset: usize,
    type_size: usize,
) -> Result<(), String> {
    let stride = if N == 0 { type_size } else { N };
    for (i, destination) in target.chunks_exact_mut(stride).enumerate() {
        // the source row is looked up in the selection at `source_offset + i`,
        // which is where the C++ `FixedSizeCopy` started its own scan
        let selection_index = source_offset
            .checked_add(i)
            .ok_or_else(|| "copy_vector_buffer: source selection index overflows".to_string())?;
        let source_row = selection.index(selection_index)?;
        let start = source_row.checked_mul(stride).ok_or_else(|| {
            "copy_vector_buffer: source range overflows the source buffer".to_string()
        })?;
        let entry = source
            .get(start..)
            .and_then(|rest| rest.get(..stride))
            .ok_or_else(|| {
                format!("copy_vector_buffer: source row {source_row} is out of range")
            })?;
        // `chunks_exact_mut` yields exactly `stride` bytes per chunk, so the
        // copy below is between two slices of the same length by construction.
        destination.copy_from_slice(entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_with_an_identity_selection() {
        let source: Vec<u8> = (0..8u8).collect();
        let mut target = [0u8; 8];
        copy_vector_buffer(&mut target, &source, &[], 0, 0, 8, 1).unwrap();
        assert_eq!(target, source[..]);
    }

    #[test]
    fn copies_through_a_selection() {
        let source: Vec<u8> = vec![10, 11, 12, 13, 14, 15];
        let selection: Vec<u32> = vec![5, 0, 3];
        let mut target = [0u8; 3];
        copy_vector_buffer(&mut target, &source, &selection, 0, 0, 3, 1).unwrap();
        assert_eq!(target, [15, 10, 13]);
    }

    #[test]
    fn copies_at_offsets() {
        let source: Vec<u8> = vec![1, 2, 3, 4];
        let selection: Vec<u32> = vec![2, 0];
        let mut target = [0u8; 4];
        // target row 1 <- source row 2, target row 2 <- source row 0
        copy_vector_buffer(&mut target, &source, &selection, 0, 1, 2, 1).unwrap();
        assert_eq!(target, [0, 3, 1, 0]);
    }

    #[test]
    fn copies_fixed_width_entries() {
        let source: Vec<u32> = vec![100, 200, 300, 400];
        let source_bytes: Vec<u8> = by_words(&source);
        let selection: Vec<u32> = vec![3, 1];
        let mut target = vec![0u8; 8];
        copy_vector_buffer(&mut target, &source_bytes, &selection, 0, 0, 2, 4).unwrap();
        assert_eq!(words(&target), vec![400, 200]);
    }

    #[test]
    fn copies_sixteen_byte_entries() {
        let source: Vec<u32> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let source_bytes = by_words(&source);
        let selection: Vec<u32> = vec![1, 0];
        let mut target = vec![0u8; 32];
        copy_vector_buffer(&mut target, &source_bytes, &selection, 0, 0, 2, 16).unwrap();
        assert_eq!(words(&target), vec![5, 6, 7, 8, 1, 2, 3, 4]);
    }

    #[test]
    fn copies_at_a_source_offset() {
        let source: Vec<u32> = vec![1, 2, 3, 4, 5];
        let source_bytes = by_words(&source);
        let mut target = vec![0u8; 8];
        // source rows 2 and 3, written from target row 0
        copy_vector_buffer(&mut target, &source_bytes, &[], 2, 0, 2, 4).unwrap();
        assert_eq!(words(&target), vec![3, 4]);
    }

    // The offset belongs to the *index* the selection is read at, not to the
    // row it names. `duckdb_vector_copy_sel(src, dst, sel, 3, 1, 2)` copies
    // two rows and reads the selection at rows 1 and 2.
    #[test]
    fn the_offset_indexes_the_selection_not_the_rows() {
        let source: Vec<u32> = vec![10, 20, 30, 40, 50, 60];
        let source_bytes = by_words(&source);
        let selection: Vec<u32> = vec![5, 3, 2, 0];
        let mut target = vec![0u8; 8];
        copy_vector_buffer(&mut target, &source_bytes, &selection, 1, 0, 2, 4).unwrap();
        // sel[1] = 3 and sel[2] = 2, so the rows are 40 and 30
        assert_eq!(words(&target), vec![40, 30]);
    }

    #[test]
    fn the_target_offset_skips_leading_rows() {
        let source: Vec<u32> = vec![10, 20, 30, 40];
        let source_bytes = by_words(&source);
        let mut target = vec![0u8; 16];
        // two rows written starting at target row 2, with the identity
        // selection
        copy_vector_buffer(&mut target, &source_bytes, &[], 1, 2, 2, 4).unwrap();
        assert_eq!(words(&target), vec![0, 0, 20, 30]);
    }

    #[test]
    fn falls_back_for_unusual_type_sizes() {
        let source: Vec<u8> = vec![1, 2, 3, 4, 5, 6];
        let selection: Vec<u32> = vec![1, 0];
        let mut target = [0u8; 6];
        copy_vector_buffer(&mut target, &source, &selection, 0, 0, 2, 3).unwrap();
        assert_eq!(target, [4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn rejects_a_target_buffer_that_is_too_small() {
        let source: Vec<u8> = vec![1, 2, 3, 4];
        let mut target = [0u8; 2];
        assert!(copy_vector_buffer(&mut target, &source, &[], 0, 0, 4, 1).is_err());
    }

    #[test]
    fn rejects_a_source_row_that_is_out_of_range() {
        let source: Vec<u8> = vec![1, 2, 3];
        let selection: Vec<u32> = vec![9];
        let mut target = [0u8; 1];
        assert!(copy_vector_buffer(&mut target, &source, &selection, 0, 0, 1, 1).is_err());
    }

    #[test]
    fn rejects_a_zero_type_size() {
        let mut target = [0u8; 4];
        assert!(copy_vector_buffer(&mut target, &[], &[], 0, 0, 4, 0).is_err());
    }

    fn by_words(values: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * 4);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn words(bytes: &[u8]) -> Vec<u32> {
        // `size_of::<u32>()` rather than the literal 4 keeps this honest about
        // which type it is decoding.
        let (chunks, _) = bytes.as_chunks::<{ std::mem::size_of::<u32>() }>();
        chunks
            .iter()
            .map(|chunk| u32::from_le_bytes(*chunk))
            .collect()
    }
}
