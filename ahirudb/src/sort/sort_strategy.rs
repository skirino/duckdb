//! Strategy choice and order materialisation ported from
//! `src/common/sort/{sort_strategy,full_sort,hashed_sort}.cpp`.

#![deny(unsafe_code)]

pub const STRATEGY_HASHED: u8 = 0;
pub const STRATEGY_FULL: u8 = 1;
pub const STRATEGY_NATURAL: u8 = 2;

use super::sort::INVALID_INDEX;

/// Strategy choice: partition_bys non-empty -> hashed, else order_bys non-empty
/// -> full, else natural.
pub fn select_strategy(has_partition_bys: bool, has_order_bys: bool) -> u8 {
    if has_partition_bys {
        STRATEGY_HASHED
    } else if has_order_bys {
        STRATEGY_FULL
    } else {
        STRATEGY_NATURAL
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderMaterialization {
    pub sort_ids_count: usize,
    pub computed_columns_count: usize,
    pub force_payload: bool,
}

/// Plan for materialising the order expressions as payload columns.
///
/// `order_input_columns[i]` is the input column the i-th order reads, or
/// `INVALID_INDEX` when the order is a computed expression.
/// `payload_column_count` is the number of input columns already in the
/// payload.
pub fn materialize_orders(
    order_input_columns: &[u64],
    payload_column_count: usize,
    require_payload: bool,
    sort_ids_out: &mut [u64],
    computed_columns_out: &mut [u64],
) -> OrderMaterialization {
    let mut sort_ids_count = 0usize;
    let mut computed_count = 0usize;
    for &col in order_input_columns {
        if col != INVALID_INDEX {
            sort_ids_out[sort_ids_count] = col;
            sort_ids_count += 1;
        } else {
            let idx = (payload_column_count + computed_count) as u64;
            sort_ids_out[sort_ids_count] = idx;
            sort_ids_count += 1;
            computed_columns_out[computed_count] = idx;
            computed_count += 1;
        }
    }

    let mut force_payload = false;
    if require_payload {
        // distinct count of sort_ids
        let mut seen: Vec<u64> = Vec::new();
        for &v in &sort_ids_out[..sort_ids_count] {
            if !seen.contains(&v) {
                seen.push(v);
            }
        }
        let payload_types_size = payload_column_count + computed_count;
        force_payload = seen.len() >= payload_types_size;
    }

    OrderMaterialization {
        sort_ids_count,
        computed_columns_count: computed_count,
        force_payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_strategy_all() {
        assert_eq!(select_strategy(true, true), STRATEGY_HASHED);
        assert_eq!(select_strategy(true, false), STRATEGY_HASHED);
        assert_eq!(select_strategy(false, true), STRATEGY_FULL);
        assert_eq!(select_strategy(false, false), STRATEGY_NATURAL);
    }

    #[test]
    fn materialize_orders_computed() {
        // one bound ref (col 2), one computed
        let orders = [2u64, INVALID_INDEX];
        let mut sort_ids = [0u64; 2];
        let mut computed = [0u64; 2];
        let r = materialize_orders(&orders, 1, true, &mut sort_ids, &mut computed);
        // sort_ids: [2, 1] (payload_count=1, computed idx=1)
        assert_eq!(sort_ids[0], 2);
        assert_eq!(sort_ids[1], 1);
        assert_eq!(computed[0], 1);
        assert_eq!(r.sort_ids_count, 2);
        assert_eq!(r.computed_columns_count, 1);
        // force_payload: sort_set {2,1} size2 >= payload_types_size (1+1=2) ->
        // true
        assert!(r.force_payload);
    }
}
