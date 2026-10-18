//! The aggregate-state lifecycle loops of `RowOperations`, mirroring
//! `src/common/row_operations/row_aggregate.cpp`.
//!
//! Every loop here walks a `TupleDataLayout`'s aggregate slots and advances a
//! `Vector` of row addresses from one aggregate state to the next. The layout
//! is projected into plain data ([`AggregateSlot`]s plus its offset array) by
//! the bridge, and the state-vector arithmetic is done with the dereferenced
//! addresses handed over as a slice. Everything that needs a DuckDB object -
//! the aggregate callbacks, `VectorOperations::AddInPlace`/`Copy`, the filtered
//! address vector - goes through [`AggregateOps`].
//!
//! # Errors
//! These functions do not report failures. A C++ callback that throws is caught
//! by the bridge trampoline and rethrown at the C++ entry point (see
//! `CallIntoRust`), so no error has to cross here.

#![deny(unsafe_code)]

use std::ffi::c_void;

//===--------------------------------------------------------------------===//
// The C-ABI function table
//===--------------------------------------------------------------------===//

/// `GetStateInitCallback()(state_input, &state_ptr, 1)` for one row.
pub type InitStateFn = extern "C" fn(aggr: *mut c_void, state_ptr: usize);
/// `GetStateDestructorCallback()(addresses, aggr_input_data, count)`.
pub type DestroyStatesFn =
    extern "C" fn(ctx: *mut c_void, aggr: *mut c_void, addresses: *mut c_void, count: usize);
/// `GetStateUpdateCallback()(inputs, aggr_input_data, input_count, addresses,
/// count)`.
pub type UpdateFn = extern "C" fn(
    ctx: *mut c_void,
    aggr: *mut c_void,
    inputs: *mut c_void,
    input_count: usize,
    addresses: *mut c_void,
    count: usize,
);
/// `GetStateClusterUpdateCallback()(inputs, aggr_input_data, input_count,
/// *clustered, count)`.
pub type ClusterUpdateFn = extern "C" fn(
    ctx: *mut c_void,
    aggr: *mut c_void,
    inputs: *mut c_void,
    input_count: usize,
    clustered: *const c_void,
    count: usize,
);
/// `GetStateCombineCallback()(sources, targets, aggr_input_data, count)`.
pub type CombineFn = extern "C" fn(
    ctx: *mut c_void,
    aggr: *mut c_void,
    sources: *mut c_void,
    targets: *mut c_void,
    count: usize,
);
/// `GetStateFinalizeCallback()(addresses, finalize_input_data, target, count,
/// offset)`.
pub type FinalizeFn = extern "C" fn(
    ctx: *mut c_void,
    aggr: *mut c_void,
    addresses: *mut c_void,
    local_state: *mut c_void,
    target: *mut c_void,
    count: usize,
    offset: usize,
);
/// `VectorOperations::AddInPlace(vector, delta)`.
pub type AddInPlaceFn = extern "C" fn(vector: *mut c_void, delta: i64);
/// `VectorOperations::Copy(source, target, count, 0, 0)`.
pub type VectorCopyFn = extern "C" fn(source: *mut c_void, target: *mut c_void, count: usize);
/// `FlatVector::SetSize(vector, size)`.
pub type SetVectorSizeFn = extern "C" fn(vector: *mut c_void, size: usize);
/// `vector.size()`.
pub type VectorSizeFn = extern "C" fn(vector: *mut c_void) -> usize;
/// `state.addresses`, created on first use.
pub type GetOrCreateAddressesFn = extern "C" fn(ctx: *mut c_void) -> *mut c_void;
/// `filter_data.ApplyFilter(payload)`.
pub type ApplyFilterFn = extern "C" fn(filter_data: *mut c_void, payload: *mut c_void) -> usize;
/// Builds `Vector(addresses, filter_data.true_sel, count)`, flattened, on the
/// heap.
pub type MakeFilteredAddressesFn =
    extern "C" fn(addresses: *mut c_void, filter_data: *mut c_void, count: usize) -> *mut c_void;
/// `&filter_data.filtered_payload`.
pub type FilteredPayloadFn = extern "C" fn(filter_data: *mut c_void) -> *mut c_void;
/// Frees a vector the table allocated (currently only the filtered-address
/// vector).
pub type FreeVectorFn = extern "C" fn(vector: *mut c_void);
/// `filter_set->GetFilterData(aggr_idx)`.
pub type GetFilterDataFn = extern "C" fn(filter_set: *mut c_void, aggr_idx: usize) -> *mut c_void;
/// `clustered.AdvanceStates(payload_size)`.
pub type ClusteredAdvanceFn = extern "C" fn(clustered: *mut c_void, payload_size: usize);
/// `&chunk.data[idx]`.
pub type ChunkChildVectorFn = extern "C" fn(chunk: *mut c_void, idx: usize) -> *mut c_void;
/// `chunk.size()`.
pub type ChunkSizeFn = extern "C" fn(chunk: *mut c_void) -> usize;
/// `state.local_states.size()`.
pub type LocalStateCountFn = extern "C" fn(ctx: *mut c_void) -> usize;
/// `state.local_states.resize(n)`.
pub type ResizeLocalStatesFn = extern "C" fn(ctx: *mut c_void, n: usize);
/// `state.local_states[idx] = init_local_state_finalize(...)`.
pub type InitLocalStateFn = extern "C" fn(ctx: *mut c_void, aggr: *mut c_void, idx: usize);
/// `state.local_states[idx].get()`.
pub type LocalStatePtrFn = extern "C" fn(ctx: *mut c_void, idx: usize) -> *mut c_void;

/// The C-ABI function table behind the aggregate lifecycle.
#[derive(Clone, Copy)]
pub struct AggregateOps {
    init_state: InitStateFn,
    destroy_states: DestroyStatesFn,
    update: UpdateFn,
    cluster_update: ClusterUpdateFn,
    combine: CombineFn,
    finalize: FinalizeFn,
    add_in_place: AddInPlaceFn,
    vector_copy: VectorCopyFn,
    set_vector_size: SetVectorSizeFn,
    vector_size: VectorSizeFn,
    get_or_create_addresses: GetOrCreateAddressesFn,
    apply_filter: ApplyFilterFn,
    make_filtered_addresses: MakeFilteredAddressesFn,
    filtered_payload: FilteredPayloadFn,
    free_vector: FreeVectorFn,
    get_filter_data: GetFilterDataFn,
    clustered_advance: ClusteredAdvanceFn,
    chunk_child_vector: ChunkChildVectorFn,
    chunk_size: ChunkSizeFn,
    local_state_count: LocalStateCountFn,
    resize_local_states: ResizeLocalStatesFn,
    init_local_state: InitLocalStateFn,
    local_state_ptr: LocalStatePtrFn,
}

impl AggregateOps {
    #[allow(clippy::too_many_arguments)]
    pub fn new_(
        init_state: InitStateFn,
        destroy_states: DestroyStatesFn,
        update: UpdateFn,
        cluster_update: ClusterUpdateFn,
        combine: CombineFn,
        finalize: FinalizeFn,
        add_in_place: AddInPlaceFn,
        vector_copy: VectorCopyFn,
        set_vector_size: SetVectorSizeFn,
        vector_size: VectorSizeFn,
        get_or_create_addresses: GetOrCreateAddressesFn,
        apply_filter: ApplyFilterFn,
        make_filtered_addresses: MakeFilteredAddressesFn,
        filtered_payload: FilteredPayloadFn,
        free_vector: FreeVectorFn,
        get_filter_data: GetFilterDataFn,
        clustered_advance: ClusteredAdvanceFn,
        chunk_child_vector: ChunkChildVectorFn,
        chunk_size: ChunkSizeFn,
        local_state_count: LocalStateCountFn,
        resize_local_states: ResizeLocalStatesFn,
        init_local_state: InitLocalStateFn,
        local_state_ptr: LocalStatePtrFn,
    ) -> Self {
        Self {
            init_state,
            destroy_states,
            update,
            cluster_update,
            combine,
            finalize,
            add_in_place,
            vector_copy,
            set_vector_size,
            vector_size,
            get_or_create_addresses,
            apply_filter,
            make_filtered_addresses,
            filtered_payload,
            free_vector,
            get_filter_data,
            clustered_advance,
            chunk_child_vector,
            chunk_size,
            local_state_count,
            resize_local_states,
            init_local_state,
            local_state_ptr,
        }
    }
}

//===--------------------------------------------------------------------===//
// The layout projection
//===--------------------------------------------------------------------===//

/// One entry of `TupleDataLayout::GetAggregates()`, projected into plain data.
///
/// `aggr` is the address of the live C++ `AggregateObject`, which every
/// callback trampoline needs; the flags are the only properties the loops
/// branch on.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AggregateSlot {
    /// The live `AggregateObject *`.
    pub aggr: *mut c_void,
    /// `aggr.payload_size`.
    pub payload_size: usize,
    /// `aggr.child_count`.
    pub child_count: usize,
    /// `aggr.function.HasStateDestructorCallback()`.
    pub has_destructor: bool,
    /// `aggr.function.GetStateClusterUpdateCallback() != nullptr`.
    pub has_cluster_update: bool,
    /// `aggr.aggr_type == AggregateType::DISTINCT`.
    pub is_distinct: bool,
    /// `aggr.filter != nullptr`.
    pub has_filter: bool,
    /// `aggr.function.HasStateCombineCallback()`.
    pub has_combine: bool,
    /// `callbacks.HasInitLocalStateFinalizeCallback()`.
    pub has_init_local_state: bool,
}

//===--------------------------------------------------------------------===//
// The seven operations
//===--------------------------------------------------------------------===//

/// `RowOperations::InitializeStates`.
///
/// `addresses_data` holds the `data_ptr_t` values the loops read: the first
/// `count` entries under the identity selection, or the address vector that
/// `sel` indexes (empty `sel` means the identity selection).
pub fn initialize_states(
    ops: AggregateOps,
    addresses_data: &[u64],
    offsets: &[usize],
    column_count: usize,
    aggregates: &[AggregateSlot],
    sel: &[u32],
    count: usize,
) {
    if count == 0
    {
        return;
    }
    for (aggregate_idx, aggr) in aggregates.iter().enumerate()
    {
        let aggr_offset = offsets[column_count + aggregate_idx];
        if sel.is_empty()
        {
            for row in 0..count
            {
                let state_ptr = addresses_data[row] as usize + aggr_offset;
                (ops.init_state)(aggr.aggr, state_ptr);
            }
        }
        else
        {
            for row in 0..count
            {
                let address_idx = sel[row] as usize;
                let state_ptr = addresses_data[address_idx] as usize + aggr_offset;
                (ops.init_state)(aggr.aggr, state_ptr);
            }
        }
    }
}

/// `RowOperations::DestroyStates`.
pub fn destroy_states(
    ops: AggregateOps,
    ctx: *mut c_void,
    aggr_offset: usize,
    aggregates: &[AggregateSlot],
    addresses: *mut c_void,
) {
    let count = (ops.vector_size)(addresses);
    if count == 0
    {
        return;
    }
    (ops.add_in_place)(addresses, aggr_offset as i64);
    for aggr in aggregates
    {
        if aggr.has_destructor
        {
            (ops.destroy_states)(ctx, aggr.aggr, addresses, count);
        }
        (ops.add_in_place)(addresses, aggr.payload_size as i64);
    }
}

/// `RowOperations::UpdateStates`.
pub fn update_states(
    ops: AggregateOps,
    ctx: *mut c_void,
    aggr: &AggregateSlot,
    addresses: *mut c_void,
    payload: *mut c_void,
    arg_idx: usize,
    clustered: *const c_void,
) {
    let count = (ops.vector_size)(addresses);
    let inputs = if aggr.child_count != 0
    {
        (ops.chunk_child_vector)(payload, arg_idx)
    }
    else
    {
        std::ptr::null_mut()
    };
    if !clustered.is_null() && aggr.has_cluster_update
    {
        (ops.cluster_update)(ctx, aggr.aggr, inputs, aggr.child_count, clustered, count);
    }
    else
    {
        (ops.update)(ctx, aggr.aggr, inputs, aggr.child_count, addresses, count);
    }
}

/// `RowOperations::UpdateFilteredStates`.
pub fn update_filtered_states(
    ops: AggregateOps,
    ctx: *mut c_void,
    filter_data: *mut c_void,
    aggr: &AggregateSlot,
    addresses: *mut c_void,
    payload: *mut c_void,
    arg_idx: usize,
) {
    let count = (ops.apply_filter)(filter_data, payload);
    if count == 0
    {
        return;
    }
    let filtered = (ops.make_filtered_addresses)(addresses, filter_data, count);
    if filtered.is_null()
    {
        // The trampoline caught a C++ exception; the boundary rethrows it.
        return;
    }
    let filtered_payload = (ops.filtered_payload)(filter_data);
    let inputs = if aggr.child_count != 0
    {
        (ops.chunk_child_vector)(filtered_payload, arg_idx)
    }
    else
    {
        std::ptr::null_mut()
    };
    (ops.update)(ctx, aggr.aggr, inputs, aggr.child_count, filtered, count);
    (ops.free_vector)(filtered);
}

/// `RowOperations::UpdateStatesClustered`.
///
/// `filter` is the `unsafe_vector<idx_t>` of aggregate indices to visit, or
/// empty when `filter_set`/`filter` is null; a null filter skips no aggregate.
#[allow(clippy::too_many_arguments)]
pub fn update_states_clustered(
    ops: AggregateOps,
    ctx: *mut c_void,
    aggregates: &[AggregateSlot],
    filter_set: *mut c_void,
    filter: &[u64],
    has_filter: bool,
    addresses: *mut c_void,
    payload: *mut c_void,
    clustered: *mut c_void,
    skip_addresses: bool,
) {
    let mut filter_idx = 0usize;
    let mut payload_idx = 0usize;
    for (aggr_idx, aggr) in aggregates.iter().enumerate()
    {
        if has_filter && (filter_idx >= filter.len() || aggr_idx < filter[filter_idx] as usize)
        {
            // Skip all the aggregates that are not in the filter
            payload_idx += aggr.child_count;
            if !skip_addresses
            {
                (ops.add_in_place)(addresses, aggr.payload_size as i64);
            }
            (ops.clustered_advance)(clustered, aggr.payload_size);
            continue;
        }

        if !aggr.is_distinct && aggr.has_filter
        {
            let filter_data = (ops.get_filter_data)(filter_set, aggr_idx);
            update_filtered_states(ops, ctx, filter_data, aggr, addresses, payload, payload_idx);
        }
        else
        {
            update_states(ops, ctx, aggr, addresses, payload, payload_idx, clustered);
        }

        // Move to the next aggregate
        payload_idx += aggr.child_count;
        if !skip_addresses
        {
            (ops.add_in_place)(addresses, aggr.payload_size as i64);
        }
        (ops.clustered_advance)(clustered, aggr.payload_size);
        if has_filter
        {
            filter_idx += 1;
        }
    }
}

/// `RowOperations::CombineStates`.
pub fn combine_states(
    ops: AggregateOps,
    ctx: *mut c_void,
    aggr_offset: usize,
    aggregates: &[AggregateSlot],
    sources: *mut c_void,
    targets: *mut c_void,
) {
    let count = (ops.vector_size)(sources);
    if count == 0
    {
        return;
    }

    // Move to the first aggregate states
    (ops.add_in_place)(sources, aggr_offset as i64);
    (ops.add_in_place)(targets, aggr_offset as i64);

    let mut offset = aggr_offset;
    for aggr in aggregates
    {
        (ops.combine)(ctx, aggr.aggr, sources, targets, count);
        (ops.add_in_place)(sources, aggr.payload_size as i64);
        (ops.add_in_place)(targets, aggr.payload_size as i64);
        offset += aggr.payload_size;
    }

    // Subtract the offset to get back to the original position
    (ops.add_in_place)(sources, -(offset as i64));
    (ops.add_in_place)(targets, -(offset as i64));
}

/// `RowOperations::FinalizeStates`.
pub fn finalize_states(
    ops: AggregateOps,
    ctx: *mut c_void,
    aggr_offset: usize,
    aggregates: &[AggregateSlot],
    addresses: *mut c_void,
    result: *mut c_void,
    aggr_idx: usize,
) {
    let addresses_copy = (ops.get_or_create_addresses)(ctx);
    if addresses_copy.is_null()
    {
        // The trampoline caught a C++ exception; the boundary rethrows it.
        return;
    }
    let result_size = (ops.chunk_size)(result);
    (ops.vector_copy)(addresses, addresses_copy, result_size);
    (ops.set_vector_size)(addresses_copy, result_size);
    (ops.add_in_place)(addresses_copy, aggr_offset as i64);

    // Initialize the finalize local states once; they are re-used across finalize
    // calls
    if (ops.local_state_count)(ctx) < aggregates.len()
    {
        (ops.resize_local_states)(ctx, aggregates.len());
        for (idx, aggr) in aggregates.iter().enumerate()
        {
            if aggr.has_init_local_state
            {
                (ops.init_local_state)(ctx, aggr.aggr, idx);
            }
        }
    }

    for (idx, aggr) in aggregates.iter().enumerate()
    {
        let target = (ops.chunk_child_vector)(result, aggr_idx + idx);
        let local_state = (ops.local_state_ptr)(ctx, idx);
        (ops.finalize)(ctx, aggr.aggr, addresses_copy, local_state, target, result_size, 0);
        (ops.set_vector_size)(target, result_size);
        (ops.add_in_place)(addresses_copy, aggr.payload_size as i64);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    // A recording table: every trampoline pushes onto a shared log instead of
    // touching DuckDB, so the loops' arithmetic can be checked on its own. The
    // log is thread-local so parallel tests do not see each other's entries.
    thread_local! {
        static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn record(entry: String) { LOG.with(|log| log.borrow_mut().push(entry)); }

    fn take_log() -> Vec<String> { LOG.with(|log| std::mem::take(&mut *log.borrow_mut())) }

    extern "C" fn init_state(aggr: *mut c_void, state_ptr: usize) {
        record(format!("init:{:p}:{}", aggr, state_ptr));
    }
    extern "C" fn destroy_states(
        _ctx: *mut c_void,
        aggr: *mut c_void,
        _addresses: *mut c_void,
        count: usize,
    ) {
        record(format!("destroy:{:p}:{count}", aggr));
    }
    extern "C" fn update(
        _ctx: *mut c_void,
        aggr: *mut c_void,
        _inputs: *mut c_void,
        input_count: usize,
        _addresses: *mut c_void,
        count: usize,
    ) {
        record(format!("update:{:p}:{input_count}:{count}", aggr));
    }
    extern "C" fn cluster_update(
        _ctx: *mut c_void,
        aggr: *mut c_void,
        _inputs: *mut c_void,
        input_count: usize,
        clustered: *const c_void,
        count: usize,
    ) {
        record(format!("cluster:{:p}:{input_count}:{:p}:{count}", aggr, clustered));
    }
    extern "C" fn combine(
        _ctx: *mut c_void,
        aggr: *mut c_void,
        _sources: *mut c_void,
        _targets: *mut c_void,
        count: usize,
    ) {
        record(format!("combine:{:p}:{count}", aggr));
    }
    extern "C" fn finalize(
        _ctx: *mut c_void,
        aggr: *mut c_void,
        _addresses: *mut c_void,
        local_state: *mut c_void,
        _target: *mut c_void,
        count: usize,
        offset: usize,
    ) {
        record(format!("finalize:{:p}:{:p}:{count}:{offset}", aggr, local_state));
    }
    extern "C" fn add_in_place(_vector: *mut c_void, delta: i64) { record(format!("add:{delta}")); }
    extern "C" fn vector_copy(_source: *mut c_void, _target: *mut c_void, count: usize) {
        record(format!("copy:{count}"));
    }
    extern "C" fn set_vector_size(_vector: *mut c_void, size: usize) {
        record(format!("setsize:{size}"));
    }
    extern "C" fn vector_size(vector: *mut c_void) -> usize {
        // The tests encode the size in the fake vector address.
        vector as usize
    }
    extern "C" fn get_or_create_addresses(_ctx: *mut c_void) -> *mut c_void {
        record("get_addresses".to_string());
        std::ptr::null_mut()
    }
    extern "C" fn apply_filter(_filter_data: *mut c_void, _payload: *mut c_void) -> usize {
        // The tests encode the filtered count in the filter-data address.
        _filter_data as usize
    }
    extern "C" fn make_filtered_addresses(
        _addresses: *mut c_void,
        _filter_data: *mut c_void,
        count: usize,
    ) -> *mut c_void {
        record(format!("make_filtered:{count}"));
        0x1000 as *mut c_void
    }
    extern "C" fn filtered_payload(_filter_data: *mut c_void) -> *mut c_void {
        0x2000 as *mut c_void
    }
    extern "C" fn free_vector(_vector: *mut c_void) { record("free".to_string()); }
    extern "C" fn get_filter_data(_filter_set: *mut c_void, aggr_idx: usize) -> *mut c_void {
        record(format!("get_filter_data:{aggr_idx}"));
        0x3000 as *mut c_void
    }
    extern "C" fn clustered_advance(_clustered: *mut c_void, payload_size: usize) {
        record(format!("advance:{payload_size}"));
    }
    extern "C" fn chunk_child_vector(_chunk: *mut c_void, idx: usize) -> *mut c_void {
        (0x4000 + idx) as *mut c_void
    }
    extern "C" fn chunk_size(chunk: *mut c_void) -> usize { chunk as usize }
    extern "C" fn local_state_count(_ctx: *mut c_void) -> usize { 0 }
    extern "C" fn resize_local_states(_ctx: *mut c_void, n: usize) {
        record(format!("resize:{n}"));
    }
    extern "C" fn init_local_state(_ctx: *mut c_void, aggr: *mut c_void, idx: usize) {
        record(format!("init_local:{:p}:{idx}", aggr));
    }
    extern "C" fn local_state_ptr(_ctx: *mut c_void, idx: usize) -> *mut c_void {
        (0x5000 + idx) as *mut c_void
    }

    fn ops() -> AggregateOps {
        AggregateOps::new_(
            init_state,
            destroy_states,
            update,
            cluster_update,
            combine,
            finalize,
            add_in_place,
            vector_copy,
            set_vector_size,
            vector_size,
            get_or_create_addresses,
            apply_filter,
            make_filtered_addresses,
            filtered_payload,
            free_vector,
            get_filter_data,
            clustered_advance,
            chunk_child_vector,
            chunk_size,
            local_state_count,
            resize_local_states,
            init_local_state,
            local_state_ptr,
        )
    }

    fn slot(aggr: usize, payload_size: usize, child_count: usize) -> AggregateSlot {
        AggregateSlot {
            aggr: aggr as *mut c_void,
            payload_size,
            child_count,
            has_destructor: false,
            has_cluster_update: false,
            is_distinct: false,
            has_filter: false,
            has_combine: true,
            has_init_local_state: false,
        }
    }

    #[test]
    fn initialize_states_advances_to_the_aggregate_offset() {
        let _ = take_log();
        // Two aggregates at offsets 16 and 24 (column_count == 1 means the
        // aggregate offsets start at index 1). Addresses 0x100, 0x200.
        let ops = ops();
        let addresses = [0x100u64, 0x200u64];
        let offsets = [8usize, 16, 24];
        let aggregates = [slot(0x10, 8, 1), slot(0x20, 8, 1)];
        initialize_states(ops, &addresses, &offsets, 1, &aggregates, &[], 2);
        let log = take_log();
        assert_eq!(
            log,
            vec![
                format!("init:{:p}:{}", 0x10 as *mut c_void, 0x110),
                format!("init:{:p}:{}", 0x10 as *mut c_void, 0x210),
                format!("init:{:p}:{}", 0x20 as *mut c_void, 0x118),
                format!("init:{:p}:{}", 0x20 as *mut c_void, 0x218),
            ]
        );
    }

    #[test]
    fn initialize_states_honors_a_selection() {
        let _ = take_log();
        let ops = ops();
        let addresses = [0x100u64, 0x200u64];
        let offsets = [8usize, 16];
        let aggregates = [slot(0x10, 8, 1)];
        // Select row 1 first, then row 0.
        initialize_states(ops, &addresses, &offsets, 1, &aggregates, &[1, 0], 2);
        let log = take_log();
        assert_eq!(
            log,
            vec![
                format!("init:{:p}:{}", 0x10 as *mut c_void, 0x210),
                format!("init:{:p}:{}", 0x10 as *mut c_void, 0x110),
            ]
        );
    }

    #[test]
    fn destroy_states_walks_every_aggregate() {
        let _ = take_log();
        let ops = ops();
        // vector_size returns the address, so 2 rows means the address 0x2.
        let mut with_dtor = slot(0x10, 8, 1);
        with_dtor.has_destructor = true;
        let aggregates = [with_dtor, slot(0x20, 16, 1)];
        super::destroy_states(ops, std::ptr::null_mut(), 4, &aggregates, 0x2 as *mut c_void);
        let log = take_log();
        assert_eq!(
            log,
            vec![
                "add:4".to_string(),
                format!("destroy:{:p}:2", 0x10 as *mut c_void),
                "add:8".to_string(),
                "add:16".to_string(),
            ]
        );
    }

    #[test]
    fn update_states_uses_the_cluster_callback_when_given_one() {
        let _ = take_log();
        let ops = ops();
        let mut aggr = slot(0x10, 8, 2);
        aggr.has_cluster_update = true;
        update_states(
            ops,
            std::ptr::null_mut(),
            &aggr,
            0x2 as *mut c_void,
            0x9 as *mut c_void,
            0,
            0x7 as *const c_void,
        );
        let log = take_log();
        assert_eq!(
            log,
            vec![format!("cluster:{:p}:2:{:p}:2", 0x10 as *mut c_void, 0x7 as *const c_void)]
        );
    }

    #[test]
    fn update_states_falls_back_to_the_plain_callback() {
        let _ = take_log();
        let ops = ops();
        let aggr = slot(0x10, 8, 0);
        update_states(
            ops,
            std::ptr::null_mut(),
            &aggr,
            0x2 as *mut c_void,
            0x9 as *mut c_void,
            0,
            std::ptr::null(),
        );
        let log = take_log();
        assert_eq!(log, vec![format!("update:{:p}:0:2", 0x10 as *mut c_void)]);
    }

    #[test]
    fn combine_states_returns_the_vectors_to_their_original_position() {
        let _ = take_log();
        let ops = ops();
        let aggregates = [slot(0x10, 8, 1), slot(0x20, 16, 1)];
        combine_states(
            ops,
            std::ptr::null_mut(),
            4,
            &aggregates,
            0x2 as *mut c_void,
            0x2 as *mut c_void,
        );
        let log = take_log();
        // add aggr_offset, then per aggregate +payload, then -total
        assert_eq!(
            log,
            vec![
                "add:4".to_string(),
                "add:4".to_string(),
                format!("combine:{:p}:2", 0x10 as *mut c_void),
                "add:8".to_string(),
                "add:8".to_string(),
                format!("combine:{:p}:2", 0x20 as *mut c_void),
                "add:16".to_string(),
                "add:16".to_string(),
                "add:-28".to_string(),
                "add:-28".to_string(),
            ]
        );
    }
}
