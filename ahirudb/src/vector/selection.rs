//! The two raw views every migrated vector algorithm reads: a selection vector
//! and a validity mask.
//!
//! Both cross the Crubit boundary as flat word slices, and both have an
//! *absent* form that means something specific in DuckDB:
//!
//! - a selection vector that is not set (`SelectionVector::IsSet() == false`)
//!   is the identity, so `get_index(i) == i`;
//! - a validity mask that cannot have nulls (`ValidityMask::CannotHaveNull() ==
//!   true`) has every row valid.
//!
//! Encoding absence as an empty slice, rather than as an `Option` or a nullable
//! pointer, is what keeps this side of the boundary free of `unsafe`: there is
//! no pointer to test, and every access goes through slice indexing that Rust
//! checks.

use std::fmt::Display;

/// A selection vector: for each output row, the source row it refers to.
///
/// An empty `entries` is the identity selection — the row refers to itself.
pub(crate) struct Selection<'a> {
    entries: &'a [u32],
}

impl<'a> Selection<'a> {
    /// Builds a view over `entries`, which is empty for the identity selection.
    pub(crate) fn new(entries: &'a [u32]) -> Self {
        Self { entries }
    }

    /// `SelectionVector::get_index`, for a row that must exist.
    pub(crate) fn index(&self, row: usize) -> Result<usize, String> {
        if self.entries.is_empty() {
            return Ok(row);
        }
        self.entries
            .get(row)
            .map(|&entry| entry as usize)
            .ok_or_else(|| format!("selection vector has no entry for row {row}"))
    }
}

/// A validity mask: one bit per row, set when the row is not NULL.
///
/// An empty `words` means the mask cannot have nulls, so every row is valid.
pub(crate) struct Validity<'a> {
    words: &'a [u64],
}

impl<'a> Validity<'a> {
    /// Builds a view over `words`, which is empty when there can be no NULLs.
    pub(crate) fn new(words: &'a [u64]) -> Self {
        Self { words }
    }

    /// `ValidityMask::RowIsValid`.
    pub(crate) fn row_is_valid(&self, row: usize) -> Result<bool, String> {
        if self.words.is_empty() {
            return Ok(true);
        }
        let word = row / u64::BITS as usize;
        let bit = row % u64::BITS as usize;
        self.words
            .get(word)
            .map(|entry| entry & (1u64 << bit) != 0)
            .ok_or_else(|| format!("validity mask has no entry for row {row}"))
    }
}

/// `UnsafeNumericCast<sel_t>`: DuckDB selects into 32-bit indexes, so a child
/// row that does not fit is a bug rather than something to truncate.
///
/// A child row is a `usize` where it comes out of an entry array and a `u64`
/// where it comes out of a `list_entry_t`, and both have to land in a `sel_t`.
pub(crate) fn sel_t<Row: TryInto<u32> + Copy + Display>(row: Row) -> Result<u32, String> {
    row.try_into()
        .map_err(|_| format!("child row {row} does not fit in a sel_t"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_selection_refers_to_each_row_itself() {
        let selection = Selection::new(&[]);
        assert_eq!(selection.index(7), Ok(7));
    }

    #[test]
    fn an_explicit_selection_maps_rows() {
        let selection = Selection::new(&[10, 20, 30]);
        assert_eq!(selection.index(0), Ok(10));
        assert_eq!(selection.index(2), Ok(30));
        assert!(selection.index(3).is_err());
    }

    #[test]
    fn an_absent_validity_mask_has_no_nulls() {
        let validity = Validity::new(&[]);
        assert_eq!(validity.row_is_valid(100), Ok(true));
    }

    #[test]
    fn a_validity_mask_reads_its_bits() {
        // rows 0 and 3 valid, rows 1 and 2 NULL
        let validity = Validity::new(&[0b1001]);
        assert!(validity.row_is_valid(0).unwrap());
        assert!(!validity.row_is_valid(1).unwrap());
        assert!(!validity.row_is_valid(2).unwrap());
        assert!(validity.row_is_valid(3).unwrap());
        assert!(validity.row_is_valid(64).is_err());
    }

    #[test]
    fn sel_t_keeps_a_child_row_that_fits() {
        assert_eq!(sel_t(7u64), Ok(7));
        assert_eq!(sel_t(7usize), Ok(7));
    }

    #[test]
    fn sel_t_rejects_a_child_row_that_does_not_fit() {
        assert!(sel_t(u64::from(u32::MAX) + 1).is_err());
        assert!(sel_t(usize::try_from(u32::MAX).unwrap() + 1).is_err());
    }
}
