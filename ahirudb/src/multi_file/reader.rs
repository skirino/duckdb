#![allow(nonstandard_style)]

//! MultiFileReader decisions: option parsing, bind planning, pre-open constants, skip/prune decisions, hive autodetect/verify, etc.

use crate::multi_file::data::{INVALID_INDEX, idx_t};

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PreOpenConstantKind {
    NONE,
    FILE_NAME,
    FILE_INDEX,
    HIVE_KEY,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PreOpenConstantResult {
    pub kind: PreOpenConstantKind,
    pub hive_key_index: idx_t,
}

/// filename → hive value extraction (smoke-test kernel)
pub fn resolve_pre_open_constant(
    column_id: idx_t,
    filename_idx: idx_t,
    _file_index: idx_t,
    hive_entries_count: idx_t,
    hive_entry_for_column: idx_t,
) -> Result<PreOpenConstantResult, String> {
    if column_id == INVALID_INDEX {
        return Err("resolve_pre_open_constant called on invalid column id".to_string());
    }

    if filename_idx != INVALID_INDEX && column_id == filename_idx {
        return Ok(PreOpenConstantResult {
            kind: PreOpenConstantKind::FILE_NAME,
            hive_key_index: INVALID_INDEX,
        });
    }

    if hive_entry_for_column < hive_entries_count {
        return Ok(PreOpenConstantResult {
            kind: PreOpenConstantKind::HIVE_KEY,
            hive_key_index: hive_entry_for_column,
        });
    }

    Ok(PreOpenConstantResult {
        kind: PreOpenConstantKind::NONE,
        hive_key_index: INVALID_INDEX,
    })
}

/// Alternative version that takes the file index column id as parameter
pub fn resolve_pre_open_constant_with_file_index(
    column_id: idx_t,
    filename_idx: idx_t,
    file_index_column_id: idx_t,
    _file_index: idx_t,
    hive_entry_for_column: idx_t,
    has_hive_entry: bool,
) -> Result<PreOpenConstantResult, String> {
    if column_id == INVALID_INDEX {
        return Err("resolve_pre_open_constant called on invalid column id".to_string());
    }

    if filename_idx != INVALID_INDEX && column_id == filename_idx {
        return Ok(PreOpenConstantResult {
            kind: PreOpenConstantKind::FILE_NAME,
            hive_key_index: INVALID_INDEX,
        });
    }

    if file_index_column_id != INVALID_INDEX && column_id == file_index_column_id {
        return Ok(PreOpenConstantResult {
            kind: PreOpenConstantKind::FILE_INDEX,
            hive_key_index: INVALID_INDEX,
        });
    }

    if has_hive_entry {
        return Ok(PreOpenConstantResult {
            kind: PreOpenConstantKind::HIVE_KEY,
            hive_key_index: hive_entry_for_column,
        });
    }

    Ok(PreOpenConstantResult {
        kind: PreOpenConstantKind::NONE,
        hive_key_index: INVALID_INDEX,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_file::data::INVALID_INDEX;

    #[test]
    fn filename_returns_file_name() {
        let res =
            resolve_pre_open_constant_with_file_index(5, 5, INVALID_INDEX, 10, 0, false).unwrap();
        assert_eq!(res.kind, PreOpenConstantKind::FILE_NAME);
    }

    #[test]
    fn file_index_returns_file_index() {
        let res =
            resolve_pre_open_constant_with_file_index(3, INVALID_INDEX, 3, 10, 0, false).unwrap();
        assert_eq!(res.kind, PreOpenConstantKind::FILE_INDEX);
    }

    #[test]
    fn hive_entry_returns_hive_key() {
        let res =
            resolve_pre_open_constant_with_file_index(7, INVALID_INDEX, INVALID_INDEX, 10, 2, true)
                .unwrap();
        assert_eq!(res.kind, PreOpenConstantKind::HIVE_KEY);
        assert_eq!(res.hive_key_index, 2);
    }

    #[test]
    fn unrelated_returns_none() {
        let res = resolve_pre_open_constant_with_file_index(7, 5, 3, 10, 0, false).unwrap();
        assert_eq!(res.kind, PreOpenConstantKind::NONE);
    }

    #[test]
    fn invalid_id_rejected() {
        let err = resolve_pre_open_constant_with_file_index(INVALID_INDEX, 5, 3, 10, 0, false)
            .unwrap_err();
        assert!(err.contains("invalid column id"));
    }
}
