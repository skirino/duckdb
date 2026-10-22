//! List vector traversal: the child-selection arithmetic of `list_entry_t`.
//!
//! A list vector stores an array of `list_entry_t` (`{offset, length}` pairs of
//! `uint64_t`) plus one flat child vector that the entries point into. Every
//! read of the child therefore starts by walking the entries, and that walk is
//! what lives here — `ListVector::GetTotalEntryCount`,
//! `ListVector::GetConsecutiveChildListInfo`,
//! `ListVector::GetConsecutiveChildSelVector` and the entry rewrite that
//! `VectorListBuffer::CopyInternal` performs while appending.
//!
//! `list_entry_t` crosses as raw bytes rather than as a struct: two `u64`s per
//! entry means a `&[u8]` over the buffer is exactly the entry array, and C++
//! hands over `data_ptr_t` without converting anything.

use crate::vector::selection::{Selection, Validity, sel_t};

/// The byte size of one `list_entry_t`.
const LIST_ENTRY_SIZE: usize = 2 * std::mem::size_of::<u64>();

/// What `ListVector::GetConsecutiveChildListInfo` learns about a slice of a
/// list vector. Mirrors `duckdb::ConsecutiveChildListInfo`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct ConsecutiveChildListInfo {
    /// Offset of the first child row the list rows refer to.
    pub child_offset: u64,
    /// Number of child rows the list rows refer to in total.
    pub child_length: u64,
    /// Whether every valid row refers to the same child range.
    pub is_constant: bool,
    /// Whether the child rows have to be gathered through a selection vector.
    pub needs_slicing: bool,
}

/// One `list_entry_t`, read out of a byte buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListEntry {
    offset: u64,
    length: u64,
}

impl ListEntry {
    /// Reads entry `index`. `entries` must hold at least `index + 1` entries.
    fn read(entries: &[u8], index: usize) -> Result<Self, String> {
        let start = index
            .checked_mul(LIST_ENTRY_SIZE)
            .ok_or_else(|| "list entries overflow the entry buffer".to_string())?;
        let entry = entries
            .get(start..)
            .and_then(|rest| rest.get(..LIST_ENTRY_SIZE))
            .ok_or_else(|| format!("list entry buffer has no entry {index}"))?;
        let (offset, length) = entry.split_at(std::mem::size_of::<u64>());
        Ok(Self {
            offset: u64::from_le_bytes(offset.try_into().unwrap()),
            length: u64::from_le_bytes(length.try_into().unwrap()),
        })
    }

    /// Writes this entry at `index`.
    fn write(&self, entries: &mut [u8], index: usize) -> Result<(), String> {
        let start = index
            .checked_mul(LIST_ENTRY_SIZE)
            .ok_or_else(|| "list entries overflow the entry buffer".to_string())?;
        let slot = entries
            .get_mut(start..)
            .and_then(|rest| rest.get_mut(..LIST_ENTRY_SIZE))
            .ok_or_else(|| format!("list entry buffer has no entry {index}"))?;
        let (offset, length) = slot.split_at_mut(std::mem::size_of::<u64>());
        offset.copy_from_slice(&self.offset.to_le_bytes());
        length.copy_from_slice(&self.length.to_le_bytes());
        Ok(())
    }
}

/// `ListVector::GetTotalEntryCount`: the number of child rows the first `count`
/// rows of a list vector refer to.
pub fn total_entry_count(
    entries: &[u8],
    selection: &[u32],
    validity: &[u64],
    count: usize,
) -> Result<u64, String> {
    let selection = Selection::new(selection);
    let validity = Validity::new(validity);
    let mut total = 0u64;
    for row in 0..count {
        if !validity.row_is_valid(selection.index(row)?)? {
            continue;
        }
        let entry = ListEntry::read(entries, selection.index(row)?)?;
        total = total
            .checked_add(entry.length)
            .ok_or_else(|| "total_entry_count: child length overflows".to_string())?;
    }
    Ok(total)
}

/// `ListVector::GetConsecutiveChildListInfo`.
pub fn consecutive_child_list_info(
    entries: &[u8],
    selection: &[u32],
    validity: &[u64],
    offset: usize,
    count: usize,
    is_constant_vector: bool,
) -> Result<ConsecutiveChildListInfo, String> {
    let selection = Selection::new(selection);
    let validity = Validity::new(validity);
    let mut info = ConsecutiveChildListInfo {
        is_constant: true,
        ..Default::default()
    };

    // The first non-NULL entry decides where the child range starts.
    let mut first_length = 0u64;
    for row in offset..offset + count {
        let mapped = selection.index(row)?;
        if !validity.row_is_valid(mapped)? {
            continue;
        }
        let entry = ListEntry::read(entries, mapped)?;
        info.child_offset = entry.offset;
        first_length = entry.length;
        break;
    }

    // A constant vector holds one entry, so there is nothing left to compare.
    if is_constant_vector {
        info.child_length = first_length;
        return Ok(info);
    }

    let mut is_consecutive = true;
    for row in offset..offset + count {
        let mapped = selection.index(row)?;
        if !validity.row_is_valid(mapped)? {
            continue;
        }
        let entry = ListEntry::read(entries, mapped)?;
        if entry.offset != info.child_offset || entry.length != first_length {
            info.is_constant = false;
        }
        let row_offset = info
            .child_offset
            .checked_add(info.child_length)
            .ok_or_else(|| "consecutive_child_list_info: child range overflows".to_string())?;
        if entry.offset != row_offset {
            is_consecutive = false;
        }
        info.child_length = info
            .child_length
            .checked_add(entry.length)
            .ok_or_else(|| "consecutive_child_list_info: child length overflows".to_string())?;
    }

    if info.is_constant {
        info.child_length = first_length;
    }
    if !info.is_constant && !is_consecutive {
        info.needs_slicing = true;
    }
    Ok(info)
}

/// `ListVector::GetConsecutiveChildSelVector`: gathers the child rows the first
/// `count` list rows refer to into `out`, and returns how many it wrote.
///
/// `out` must hold at least as many entries as those rows refer to in total;
/// `consecutive_child_list_info` reports that count.
pub fn consecutive_child_sel_vector(
    entries: &[u8],
    selection: &[u32],
    validity: &[u64],
    offset: usize,
    count: usize,
    out: &mut [u32],
) -> Result<usize, String> {
    let selection = Selection::new(selection);
    let validity = Validity::new(validity);
    let mut written = 0usize;
    for row in offset..offset + count {
        let mapped = selection.index(row)?;
        if !validity.row_is_valid(mapped)? {
            continue;
        }
        let entry = ListEntry::read(entries, mapped)?;
        for child in 0..entry.length {
            let slot = out.get_mut(written).ok_or_else(|| {
                "consecutive_child_sel_vector: selection vector is too small".to_string()
            })?;
            *slot = sel_t(entry.offset + child)?;
            written += 1;
        }
    }
    Ok(written)
}

/// The row range a list copy moves, which `VectorBuffer::Copy` already
/// computes.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CopyRange {
    /// The first row the source selection is read at.
    pub source_offset: usize,
    /// The first row the target entries are written at.
    pub target_offset: usize,
    /// How many rows to move.
    pub count: usize,
}

/// The entry rewrite half of `VectorListBuffer::CopyInternal`.
///
/// Re-points the target entries in `range` at the child rows the caller is
/// about to append, starting at `current_child_len`. Rows that are NULL in the
/// target are left alone, exactly as in C++.
///
/// Returns the child rows to append, in the order they must be appended. The
/// caller owns the child vector, so the append itself stays in C++; everything
/// that touches the entry arrays happens here.
pub fn copy_list_entries(
    source_entries: &[u8],
    source_sel: &[u32],
    target_entries: &mut [u8],
    target_validity: &[u64],
    range: CopyRange,
    current_child_len: u64,
) -> Result<Vec<u32>, String> {
    let selection = Selection::new(source_sel);
    let validity = Validity::new(target_validity);
    let mut child_rows = Vec::new();
    let mut child_len = current_child_len;
    for i in 0..range.count {
        let target_row = range.target_offset + i;
        if !validity.row_is_valid(target_row)? {
            continue;
        }
        let source = ListEntry::read(source_entries, selection.index(range.source_offset + i)?)?;
        for child in 0..source.length {
            child_rows.push(sel_t(source.offset + child)?);
        }
        ListEntry {
            offset: child_len,
            length: source.length,
        }
        .write(target_entries, target_row)?;
        child_len = child_len
            .checked_add(source.length)
            .ok_or_else(|| "copy_list_entries: child length overflows".to_string())?;
    }
    Ok(child_rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(offsets_and_lengths: &[(u64, u64)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for &(offset, length) in offsets_and_lengths {
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&length.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn total_entry_count_sums_valid_rows() {
        let bytes = entries(&[(0, 2), (2, 3), (5, 1)]);
        assert_eq!(total_entry_count(&bytes, &[], &[], 3), Ok(6));
        // only the first two rows
        assert_eq!(total_entry_count(&bytes, &[], &[], 2), Ok(5));
    }

    #[test]
    fn total_entry_count_skips_null_rows() {
        let bytes = entries(&[(0, 2), (2, 3), (5, 1)]);
        // row 1 is NULL
        let validity = [0b101u64];
        assert_eq!(total_entry_count(&bytes, &[], &validity, 3), Ok(3));
    }

    #[test]
    fn total_entry_count_follows_a_selection() {
        let bytes = entries(&[(0, 2), (2, 3), (5, 1)]);
        let selection = [2u32, 0u32];
        assert_eq!(total_entry_count(&bytes, &selection, &[], 2), Ok(3));
    }

    #[test]
    fn consecutive_child_list_info_of_a_consecutive_slice() {
        let bytes = entries(&[(0, 2), (2, 3), (5, 4)]);
        let info = consecutive_child_list_info(&bytes, &[], &[], 0, 3, false).unwrap();
        assert_eq!(
            info,
            ConsecutiveChildListInfo {
                child_offset: 0,
                child_length: 9,
                // the rows are back to back but not the same range, so the
                // child can be read as one slice without slicing it
                is_constant: false,
                needs_slicing: false,
            }
        );
    }

    #[test]
    fn same_range_every_row_is_constant() {
        let bytes = entries(&[(0, 2), (0, 2), (0, 2)]);
        let info = consecutive_child_list_info(&bytes, &[], &[], 0, 3, false).unwrap();
        assert!(info.is_constant);
        assert!(!info.needs_slicing);
        // a constant length collapses to one range
        assert_eq!(info.child_length, 2);
    }

    #[test]
    fn consecutive_child_list_info_skips_null_rows() {
        let bytes = entries(&[(10, 2), (2, 3), (5, 4)]);
        // the row that would break consecutiveness is NULL
        let validity = [0b101u64];
        let info = consecutive_child_list_info(&bytes, &[], &validity, 0, 3, false).unwrap();
        assert_eq!(info.child_offset, 10);
        assert_eq!(info.child_length, 6);
    }

    #[test]
    fn consecutive_child_list_info_detects_a_gap() {
        let bytes = entries(&[(0, 2), (10, 3)]);
        let info = consecutive_child_list_info(&bytes, &[], &[], 0, 2, false).unwrap();
        assert!(!info.is_constant);
        assert!(info.needs_slicing);
        assert_eq!(info.child_offset, 0);
        assert_eq!(info.child_length, 5);
    }

    #[test]
    fn consecutive_child_list_info_of_a_constant_vector_stops_early() {
        let bytes = entries(&[(4, 6), (4, 6)]);
        let info = consecutive_child_list_info(&bytes, &[], &[], 0, 2, true).unwrap();
        assert_eq!(info.child_offset, 4);
        assert_eq!(info.child_length, 6);
    }

    #[test]
    fn consecutive_child_sel_vector_gathers_child_rows() {
        let bytes = entries(&[(0, 2), (2, 3)]);
        let mut out = [0u32; 5];
        let written = consecutive_child_sel_vector(&bytes, &[], &[], 0, 2, &mut out).unwrap();
        assert_eq!(written, 5);
        assert_eq!(out, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn consecutive_child_sel_vector_skips_null_rows() {
        let bytes = entries(&[(0, 2), (2, 3)]);
        let validity = [0b01u64];
        let mut out = [0u32; 2];
        let written = consecutive_child_sel_vector(&bytes, &[], &validity, 0, 2, &mut out).unwrap();
        assert_eq!(written, 2);
        assert_eq!(out, [0, 1]);
    }

    #[test]
    fn consecutive_child_sel_vector_rejects_a_small_buffer() {
        let bytes = entries(&[(0, 2), (2, 3)]);
        let mut out = [0u32; 3];
        assert!(consecutive_child_sel_vector(&bytes, &[], &[], 0, 2, &mut out).is_err());
    }

    #[test]
    fn copy_list_entries_repoints_every_valid_row() {
        let source = entries(&[(10, 2), (20, 1)]);
        let mut target = entries(&[(0, 0), (0, 0), (0, 0)]);
        let range = CopyRange {
            source_offset: 0,
            target_offset: 1,
            count: 2,
        };
        let child_rows = copy_list_entries(&source, &[], &mut target, &[], range, 100).unwrap();
        assert_eq!(child_rows, vec![10, 11, 20]);
        // valid rows are re-pointed at the appended range
        assert_eq!(
            ListEntry::read(&target, 1).unwrap(),
            ListEntry {
                offset: 100,
                length: 2
            }
        );
        assert_eq!(
            ListEntry::read(&target, 2).unwrap(),
            ListEntry {
                offset: 102,
                length: 1
            }
        );
    }

    #[test]
    fn copy_list_entries_leaves_null_target_rows_alone() {
        let source = entries(&[(10, 2), (20, 1)]);
        let mut target = entries(&[(0, 0), (7, 7), (0, 0)]);
        // row 1 of the target is NULL (rows 0 and 2 are valid)
        let validity = [0b101u64];
        let range = CopyRange {
            source_offset: 0,
            target_offset: 1,
            count: 2,
        };
        let child_rows = copy_list_entries(&source, &[], &mut target, &validity, range, 0).unwrap();
        assert_eq!(child_rows, vec![20]);
        assert_eq!(
            ListEntry::read(&target, 1).unwrap(),
            ListEntry {
                offset: 7,
                length: 7
            }
        );
        assert_eq!(
            ListEntry::read(&target, 2).unwrap(),
            ListEntry {
                offset: 0,
                length: 1
            }
        );
    }
}
