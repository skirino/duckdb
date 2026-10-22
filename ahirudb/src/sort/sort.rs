//! Layout derivation ported from `src/common/sort/sort.cpp` (constructor).

#![deny(unsafe_code)]

pub const INVALID_INDEX: u64 = u64::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortLayout {
    pub projection_column_count: usize,
    pub input_projection_map_count: usize,
    pub payload_column_count: usize,
}

pub fn derive_sort_layout(
    order_input_columns: &[u64],
    projection_map: &[u64],
    projection_columns_out: &mut [u64],
    input_projection_map_out: &mut [u64],
) -> SortLayout {
    // input column -> key idx (first wins)
    let mut input_column_to_key: Vec<(u64, usize)> = Vec::new();
    for (key_idx, &col) in order_input_columns.iter().enumerate() {
        if col == INVALID_INDEX {
            continue;
        }
        if !input_column_to_key.iter().any(|(c, _)| *c == col) {
            input_column_to_key.push((col, key_idx));
        }
    }

    let mut payload_count: usize = 0;
    let mut cols: Vec<(u64, u64, u64)> = Vec::new();
    for (output_col_idx, &input_col_idx) in projection_map.iter().enumerate() {
        let key = input_column_to_key
            .iter()
            .find(|(c, _)| *c == input_col_idx);
        if let Some((_, key_idx)) = key {
            cols.push((0, *key_idx as u64, output_col_idx as u64));
        } else {
            cols.push((1, payload_count as u64, output_col_idx as u64));
            input_projection_map_out[payload_count] = input_col_idx;
            payload_count += 1;
        }
    }

    cols.sort_by(|a, b| {
        if a.0 != b.0 {
            a.0.cmp(&b.0)
        } else {
            a.1.cmp(&b.1)
        }
    });

    for (i, (ip, lci, oci)) in cols.into_iter().enumerate() {
        let base = i * 3;
        if base + 2 < projection_columns_out.len() {
            projection_columns_out[base] = ip;
            projection_columns_out[base + 1] = lci;
            projection_columns_out[base + 2] = oci;
        }
    }

    SortLayout {
        projection_column_count: projection_map.len(),
        input_projection_map_count: payload_count,
        payload_column_count: payload_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_layout_basic() {
        let order_input = [0u64];
        let proj_map = [0u64, 1u64];
        let mut out = [0u64; 6];
        let mut ipm = [0u64; 2];
        let r = derive_sort_layout(&order_input, &proj_map, &mut out, &mut ipm);
        assert_eq!(r.projection_column_count, 2);
        assert_eq!(r.payload_column_count, 1);
        assert_eq!(r.input_projection_map_count, 1);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 0);
        assert_eq!(out[2], 0);
        assert_eq!(out[3], 1);
        assert_eq!(out[4], 0);
        assert_eq!(out[5], 1);
        assert_eq!(ipm[0], 1);
    }

    #[test]
    fn derive_layout_duplicate_key() {
        // order on col 0; projection has col 0 twice? Actually projection_map
        // distinct usually. Test computed order (INVALID_INDEX) doesn't
        // create input_column_to_key entry.
        let order_input = [INVALID_INDEX];
        let proj_map = [0u64, 1u64];
        let mut out = [0u64; 6];
        let mut ipm = [0u64; 2];
        let r = derive_sort_layout(&order_input, &proj_map, &mut out, &mut ipm);
        assert_eq!(r.payload_column_count, 2);
    }
}
