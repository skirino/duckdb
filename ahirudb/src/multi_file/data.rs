//! The multi-file reader's index vocabulary.
//!
//! Ported from `duckdb/common/multi_file/multi_file_data.hpp`.
//!
//! The column mapper talks about a column of one file
//! (`MultiFileLocalColumnId`), a projected position within that file's column
//! list (`MultiFileLocalIndex`), and a slot in the global schema
//! (`MultiFileGlobalIndex`). Each is an `idx_t` with a distinct meaning, and
//! mixing two of them up is the mistake the distinct types prevent - so what
//! crossed is what all three share: the position itself, `INVALID_INDEX` for
//! "no position", and the rules for reading one back.
//!
//! # What is here and what is not
//!
//! The rules the C++ index types enforce - that an invalid index cannot be
//! unwrapped, that a filter entry is either a column or a constant and never
//! both, that a projection position is inside the list it indexes - are here,
//! as functions over the values the C++ structs hold. Their *storage* stays in
//! C++: `multi_file_data.hpp` is a public header that every reader extension
//! includes, and a public header cannot include `crubit/ahirudb.h` (only the
//! sources in this directory may, which is why `multi_file_rust_bridge.hpp` is
//! private to them). So the C++ structs keep their `idx_t` members, and what
//! they no longer keep is the arithmetic.
//!
//! Nothing here panics: an index that is invalid or out of range is an `Err`,
//! and the bridge turns it into the `InternalException` the original code
//! threw.

// The C++ spellings are kept, so `idx_t` and `INVALID_INDEX` read the same on both sides.
#![allow(nonstandard_style)]

/// The type DuckDB uses for a size, a count or a position.
pub type idx_t = u64;

/// `DConstants::INVALID_INDEX`: a value that is never a position, so an index
/// equal to it means "no index". It is what the C++ constructors of the index
/// types default to, which is how "not found" is spelled.
pub const INVALID_INDEX: idx_t = idx_t::MAX;

/// The three fields the Rust rules below read from a `MultiFileFilterEntry`, in
/// the order the C++ struct declares them.
///
/// The struct itself stays in the public header; the bridge copies its members
/// into this value and back.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MultiFileFilterEntryLayout {
    pub is_set: bool,
    pub is_constant: bool,
    /// A constant-map index if `is_constant`, a local index otherwise - the C++
    /// packs both into one `idx_t` the same way.
    pub index: idx_t,
}

/// Why [`Default`] here is written out rather than derived: the default is a
/// default-constructed `MultiFileFilterEntry`, whose `index` is
/// `INVALID_INDEX`. A derived `Default` would give `0`, which is a position.
impl Default for MultiFileFilterEntryLayout {
    fn default() -> Self { Self::new(false, false, INVALID_INDEX) }
}

impl MultiFileFilterEntryLayout {
    /// An entry of either kind: `is_set` says whether it was resolved at all,
    /// `is_constant` which of the two kinds it is, and `index` is then the
    /// constant-map index or the local index.
    pub fn new(is_set: bool, is_constant: bool, index: idx_t) -> Self {
        Self { is_set, is_constant, index }
    }
}

//===--------------------------------------------------------------------===//
// MultiFileLocalIndex
//===--------------------------------------------------------------------===//

/// Whether `index` names a position, i.e. whether it is not `INVALID_INDEX`.
pub fn local_index_is_valid(index: idx_t) -> bool { index != INVALID_INDEX }

/// The position `index` names.
///
/// The C++ `MultiFileLocalIndex::GetIndex` threw when it did not name one,
/// which is what a mapper that failed to find a column turns into. A throw
/// cannot cross the boundary, so this is an error value and the bridge throws.
pub fn local_index_get_index(index: idx_t) -> Result<idx_t, String> {
    if !local_index_is_valid(index)
    {
        return Err("MultiFileLocalIndex::GetIndex called on invalid index".to_string());
    }
    Ok(index)
}

/// Resolves a projection position against a column list, refusing one that is
/// invalid or out of range.
///
/// The C++ `MultiFileLocalColumnIds::operator[]` indexed its `vector` with the
/// position directly. A position that is invalid, or one past the end, read out
/// of bounds; both are refused here instead, and the caller indexes only what
/// came back. The return value is the position to index into the column list.
pub fn local_column_ids_get(size: idx_t, index: idx_t) -> Result<idx_t, String> {
    let index = local_index_get_index(index)?;
    if index >= size
    {
        return Err(format!("MultiFileLocalColumnIds index {index} out of range"));
    }
    Ok(index)
}

//===--------------------------------------------------------------------===//
// MultiFileFilterEntry
//===--------------------------------------------------------------------===//

/// The entry for a global column that resolves to a constant.
pub fn filter_entry_set_constant(constant_index: idx_t) -> MultiFileFilterEntryLayout {
    MultiFileFilterEntryLayout::new(true, true, constant_index)
}

/// The entry for a global column that resolves to a column of this file.
pub fn filter_entry_set_local(local_index: idx_t) -> MultiFileFilterEntryLayout {
    MultiFileFilterEntryLayout::new(true, false, local_index)
}

/// Whether the entry is a constant.
///
/// The C++ asserted that it was set at all before answering; an unset entry has
/// neither kind, so this is the error.
pub fn filter_entry_is_constant(entry: MultiFileFilterEntryLayout) -> Result<bool, String> {
    if !entry.is_set
    {
        return Err("MultiFileFilterEntry::IsConstant called on an unset entry".to_string());
    }
    Ok(entry.is_constant)
}

/// The constant-map index of an entry that is a constant.
pub fn filter_entry_get_constant_index(entry: MultiFileFilterEntryLayout) -> Result<idx_t, String> {
    if !filter_entry_is_constant(entry)?
    {
        return Err("MultiFileFilterEntry::GetConstantIndex called on a local entry".to_string());
    }
    Ok(entry.index)
}

/// The local index of an entry that is a column of this file.
pub fn filter_entry_get_local_index(entry: MultiFileFilterEntryLayout) -> Result<idx_t, String> {
    if !filter_entry_is_constant(entry)?
    {
        return Ok(entry.index);
    }
    Err("MultiFileFilterEntry::GetLocalIndex called on a constant entry".to_string())
}

/// The position of a filter-map entry, or the error for a global index the map
/// does not cover.
///
/// The C++ `MultiFileFilterMap::operator[]` indexed its `vector` with the
/// global index directly. The return value is the index of the filter map entry
/// to use.
pub fn filter_map_get(size: idx_t, global_index: idx_t) -> Result<idx_t, String> {
    if global_index >= size
    {
        return Err(format!("MultiFileFilterMap index {global_index} out of range"));
    }
    Ok(global_index)
}

/// The position of a constant-map entry, or the error for a constant-map index
/// the map does not cover.
///
/// The C++ `MultiFileConstantMap::operator[]` indexed its `vector` with the
/// constant-map index directly.
pub fn constant_map_get(size: idx_t, constant_index: idx_t) -> Result<idx_t, String> {
    if constant_index >= size
    {
        return Err(format!("MultiFileConstantMap index {constant_index} out of range"));
    }
    Ok(constant_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_of_invalid_index_names_no_position() {
        assert!(local_index_is_valid(0));
        assert_eq!(local_index_get_index(0), Ok(0));
        assert_eq!(local_index_get_index(42), Ok(42));

        assert!(!local_index_is_valid(INVALID_INDEX));
        assert_eq!(
            local_index_get_index(INVALID_INDEX).unwrap_err(),
            "MultiFileLocalIndex::GetIndex called on invalid index"
        );
    }

    #[test]
    fn a_column_list_refuses_a_position_it_does_not_cover() {
        assert_eq!(local_column_ids_get(2, 0), Ok(0));
        assert_eq!(local_column_ids_get(2, 1), Ok(1));
        assert_eq!(
            local_column_ids_get(2, 2).unwrap_err(),
            "MultiFileLocalColumnIds index 2 out of range"
        );
        assert_eq!(
            local_column_ids_get(0, 0).unwrap_err(),
            "MultiFileLocalColumnIds index 0 out of range"
        );
        // an invalid position is refused before it is used as one
        assert_eq!(
            local_column_ids_get(2, INVALID_INDEX).unwrap_err(),
            "MultiFileLocalIndex::GetIndex called on invalid index"
        );
    }

    #[test]
    fn a_filter_entry_is_unset_until_it_is_set() {
        let entry = MultiFileFilterEntryLayout::default();
        assert!(!entry.is_set);
        // the index of an entry that is set is INVALID_INDEX, as the C++
        // default constructor spells it
        assert_eq!(entry.index, INVALID_INDEX);
        assert!(filter_entry_is_constant(entry).is_err());
        assert!(filter_entry_get_constant_index(entry).is_err());
        assert!(filter_entry_get_local_index(entry).is_err());
    }

    #[test]
    fn a_filter_entry_is_either_a_column_or_a_constant() {
        let local = filter_entry_set_local(3);
        assert!(local.is_set);
        assert_eq!(filter_entry_is_constant(local), Ok(false));
        assert_eq!(filter_entry_get_local_index(local), Ok(3));
        assert!(filter_entry_get_constant_index(local).is_err());

        let constant = filter_entry_set_constant(4);
        assert!(constant.is_set);
        assert_eq!(filter_entry_is_constant(constant), Ok(true));
        assert_eq!(filter_entry_get_constant_index(constant), Ok(4));
        assert!(filter_entry_get_local_index(constant).is_err());
    }

    #[test]
    fn a_filter_entry_holds_one_index_of_either_kind() {
        let local = filter_entry_set_local(3);
        let constant = filter_entry_set_constant(3);
        // the two kinds are told apart by more than their index
        assert!(filter_entry_get_local_index(local).is_ok());
        assert!(filter_entry_get_constant_index(constant).is_ok());
    }

    #[test]
    fn a_filter_map_refuses_a_global_column_it_does_not_cover() {
        assert_eq!(filter_map_get(2, 0), Ok(0));
        assert_eq!(filter_map_get(2, 1), Ok(1));
        assert_eq!(filter_map_get(2, 2).unwrap_err(), "MultiFileFilterMap index 2 out of range");
        assert_eq!(filter_map_get(0, 0).unwrap_err(), "MultiFileFilterMap index 0 out of range");
    }

    #[test]
    fn a_constant_map_refuses_an_index_it_does_not_cover() {
        assert_eq!(constant_map_get(2, 0), Ok(0));
        assert_eq!(constant_map_get(2, 1), Ok(1));
        assert_eq!(
            constant_map_get(2, 2).unwrap_err(),
            "MultiFileConstantMap index 2 out of range"
        );
        assert_eq!(
            constant_map_get(0, 0).unwrap_err(),
            "MultiFileConstantMap index 0 out of range"
        );
    }
}
