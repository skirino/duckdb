//! `RowMatcher`: the match-function construction and the matching loops,
//! mirroring `src/common/row_operations/row_matcher.cpp`.
//!
//! The C++ version builds a tree of `MatchFunction`s, one per predicate column,
//! and then runs each against the RHS rows. The tree construction and the row
//! loops live here; the leaf work that needs a live DuckDB object - loading a
//! templated value and comparing it, building the struct row-location vector,
//! gathering for the list path - is reached through [`MatcherOps`].
//!
//! The tree has three node kinds, exactly the three the C++ dispatch produces:
//!
//! - [`MatchNode::Primitive`] is `TemplatedMatch<T, OP>`: the fixed-width and
//!   string comparisons, where each row's LHS entry is compared to the RHS row
//!   bytes.
//! - [`MatchNode::StructEquality`] is `StructMatchEquality<OP>`: STRUCT
//!   equality, which keeps the rows whose nullness matches and then recurses
//!   into the struct's children.
//! - [`MatchNode::Generic`] is `GenericNestedMatch<OP>`: the LIST/ARRAY path
//!   and the UNION equality path, which gather the RHS column into a dense
//!   vector and use `VectorOperations::SelectComparison`.
//!
//! # Errors
//! Unlike the aggregate loops, `Initialize` can fail (an unsupported physical
//! type or predicate, or a `columns`/`predicates` size mismatch), where the C++
//! threw. Those failures come back as `Err` and the bridge turns them into the
//! same exception. A callback that throws is caught by the trampoline and
//! rethrown at the entry point, as in `row_aggregate`.

#![deny(unsafe_code)]

use std::ffi::c_void;

//===--------------------------------------------------------------------===//
// Enumerations, mirrored from the C++
//===--------------------------------------------------------------------===//

/// `PhysicalType` (`duckdb/common/types.hpp`).
mod physical {
    pub const BOOL: u8 = 1;
    pub const UINT8: u8 = 2;
    pub const INT8: u8 = 3;
    pub const UINT16: u8 = 4;
    pub const INT16: u8 = 5;
    pub const UINT32: u8 = 6;
    pub const INT32: u8 = 7;
    pub const UINT64: u8 = 8;
    pub const INT64: u8 = 9;
    pub const FLOAT: u8 = 11;
    pub const DOUBLE: u8 = 12;
    pub const INTERVAL: u8 = 21;
    pub const LIST: u8 = 23;
    pub const STRUCT: u8 = 24;
    pub const ARRAY: u8 = 29;
    pub const VARCHAR: u8 = 200;
    pub const UINT128: u8 = 203;
    pub const INT128: u8 = 204;
}

/// `ExpressionType` (`duckdb/common/enums/expression_type.hpp`).
mod expression_type {
    pub const COMPARE_EQUAL: u8 = 25;
    pub const COMPARE_NOTEQUAL: u8 = 26;
    pub const COMPARE_LESSTHAN: u8 = 27;
    pub const COMPARE_GREATERTHAN: u8 = 28;
    pub const COMPARE_LESSTHANOREQUALTO: u8 = 29;
    pub const COMPARE_GREATERTHANOREQUALTO: u8 = 30;
    pub const COMPARE_DISTINCT_FROM: u8 = 37;
    pub const COMPARE_NOT_DISTINCT_FROM: u8 = 40;
}

//===--------------------------------------------------------------------===//
// The C-ABI function table
//===--------------------------------------------------------------------===//

/// `layout.ColumnCount()`.
pub type LayoutColumnCountFn = extern "C" fn(layout: *const c_void) -> usize;
/// `layout.CanHaveNull()`.
pub type LayoutCanHaveNullFn = extern "C" fn(layout: *const c_void) -> bool;
/// `layout.GetOffsets()[col_idx]`.
pub type LayoutOffsetFn = extern "C" fn(layout: *const c_void, col_idx: usize) -> usize;
/// `&layout.GetTypes()[col_idx]`.
pub type LayoutTypeFn = extern "C" fn(layout: *const c_void, col_idx: usize) -> *const c_void;
/// `&layout.GetStructLayout(col_idx)`.
pub type LayoutStructFn = extern "C" fn(layout: *const c_void, col_idx: usize) -> *const c_void;
/// `type.InternalType()`.
pub type TypePhysicalFn = extern "C" fn(ty: *const c_void) -> u8;
/// `type.id() == LogicalTypeId::UNION`.
pub type TypeIsUnionFn = extern "C" fn(ty: *const c_void) -> bool;
/// `StructType::GetChildCount(type)`.
pub type TypeStructChildCountFn = extern "C" fn(ty: *const c_void) -> usize;
/// `&StructType::GetChildTypes(type)[idx].second`.
pub type TypeStructChildFn = extern "C" fn(ty: *const c_void, idx: usize) -> *const c_void;
/// `&lhs_formats[col_idx]`.
pub type LhsFormatAtFn = extern "C" fn(formats: *const c_void, col_idx: usize) -> *const c_void;
/// `&lhs_format.children[col_idx]`.
pub type LhsFormatChildFn = extern "C" fn(format: *const c_void, col_idx: usize) -> *const c_void;
/// `lhs_format.unified.validity.CannotHaveNull()`.
pub type LhsCannotHaveNullFn = extern "C" fn(format: *const c_void) -> bool;
/// `!validity.RowIsValid(*sel->get_index(idx))`, with the sel's null identity.
pub type LhsRowIsNullFn = extern "C" fn(format: *const c_void, idx: usize) -> bool;
/// `TemplatedMatchLoop`'s per-row comparison.
pub type CompareRowFn = extern "C" fn(
    format: *const c_void,
    idx: usize,
    predicate: u8,
    physical: u8,
    rhs_ptr: usize,
    rhs_null: bool,
    lhs_all_valid: bool,
) -> bool;
/// `ValidityBytes::RowIsValid(...)` for the RHS row at `location`.
pub type RhsRowIsNullFn = extern "C" fn(
    layout: *const c_void,
    location: usize,
    col_idx: usize,
    column_count: usize,
) -> bool;
/// `rhs_row_locations[idx]`.
pub type RhsLocationAtFn = extern "C" fn(row_locations: *mut c_void, idx: usize) -> usize;
/// `StructMatchEquality`'s null keep condition
/// (`COMPARISON_OP::COMPARE_NULL && Operation<uint32_t>(0, 0, lnull, rnull)`).
pub type StructNullKeepFn = extern "C" fn(predicate: u8, lhs_null: bool, rhs_null: bool) -> bool;
/// Builds the `Vector` of STRUCT row locations, on the heap.
pub type BuildStructRowLocationsFn = extern "C" fn(
    row_locations: *mut c_void,
    sel: *const u32,
    match_count: usize,
    offset: usize,
) -> *mut c_void;
/// Frees a vector the table allocated.
pub type FreeVectorFn = extern "C" fn(vector: *mut c_void);
/// `StructVector::GetEntries(lhs_vector).data()`.
pub type StructEntriesFn = extern "C" fn(lhs_vector: *mut c_void) -> *mut c_void;
/// `&((Vector *) entries)[idx]`.
pub type StructEntryFn = extern "C" fn(entries: *mut c_void, idx: usize) -> *mut c_void;
/// `GenericNestedMatch`: gather + `SelectComparison`. Returns `match_count` and
/// writes the updated `no_match_count` through the final pointer.
#[allow(clippy::too_many_arguments)]
pub type GenericNestedMatchFn = extern "C" fn(
    predicate: u8,
    rhs_layout: *const c_void,
    col_idx: usize,
    lhs_vector: *mut c_void,
    rhs_row_locations: *mut c_void,
    sel: *mut u32,
    count: usize,
    no_match_sel: *mut u32,
    no_match_sel_capacity: usize,
    no_match_count: *mut usize,
) -> usize;
/// `&chunk.data[col_idx]`.
pub type ChunkChildVectorFn = extern "C" fn(chunk: *mut c_void, col_idx: usize) -> *mut c_void;

/// The C-ABI function table behind `RowMatcher`.
#[derive(Clone, Copy)]
pub struct MatcherOps {
    layout_column_count: LayoutColumnCountFn,
    layout_can_have_null: LayoutCanHaveNullFn,
    layout_offset: LayoutOffsetFn,
    layout_type: LayoutTypeFn,
    layout_struct: LayoutStructFn,
    type_physical: TypePhysicalFn,
    type_is_union: TypeIsUnionFn,
    type_struct_child_count: TypeStructChildCountFn,
    type_struct_child: TypeStructChildFn,
    lhs_format_at: LhsFormatAtFn,
    lhs_format_child: LhsFormatChildFn,
    lhs_cannot_have_null: LhsCannotHaveNullFn,
    lhs_row_is_null: LhsRowIsNullFn,
    compare_row: CompareRowFn,
    rhs_row_is_null: RhsRowIsNullFn,
    rhs_location_at: RhsLocationAtFn,
    struct_null_keep: StructNullKeepFn,
    build_struct_row_locations: BuildStructRowLocationsFn,
    free_vector: FreeVectorFn,
    struct_entries: StructEntriesFn,
    struct_entry: StructEntryFn,
    generic_nested_match: GenericNestedMatchFn,
    chunk_child_vector: ChunkChildVectorFn,
}

impl MatcherOps {
    #[allow(clippy::too_many_arguments)]
    pub fn new_(
        layout_column_count: LayoutColumnCountFn,
        layout_can_have_null: LayoutCanHaveNullFn,
        layout_offset: LayoutOffsetFn,
        layout_type: LayoutTypeFn,
        layout_struct: LayoutStructFn,
        type_physical: TypePhysicalFn,
        type_is_union: TypeIsUnionFn,
        type_struct_child_count: TypeStructChildCountFn,
        type_struct_child: TypeStructChildFn,
        lhs_format_at: LhsFormatAtFn,
        lhs_format_child: LhsFormatChildFn,
        lhs_cannot_have_null: LhsCannotHaveNullFn,
        lhs_row_is_null: LhsRowIsNullFn,
        compare_row: CompareRowFn,
        rhs_row_is_null: RhsRowIsNullFn,
        rhs_location_at: RhsLocationAtFn,
        struct_null_keep: StructNullKeepFn,
        build_struct_row_locations: BuildStructRowLocationsFn,
        free_vector: FreeVectorFn,
        struct_entries: StructEntriesFn,
        struct_entry: StructEntryFn,
        generic_nested_match: GenericNestedMatchFn,
        chunk_child_vector: ChunkChildVectorFn,
    ) -> Self {
        Self {
            layout_column_count,
            layout_can_have_null,
            layout_offset,
            layout_type,
            layout_struct,
            type_physical,
            type_is_union,
            type_struct_child_count,
            type_struct_child,
            lhs_format_at,
            lhs_format_child,
            lhs_cannot_have_null,
            lhs_row_is_null,
            compare_row,
            rhs_row_is_null,
            rhs_location_at,
            struct_null_keep,
            build_struct_row_locations,
            free_vector,
            struct_entries,
            struct_entry,
            generic_nested_match,
            chunk_child_vector,
        }
    }
}

//===--------------------------------------------------------------------===//
// The match tree and the per-matcher state
//===--------------------------------------------------------------------===//

/// One node of the match-function tree.
enum MatchNode {
    /// `TemplatedMatch<T, OP>` for a fixed-width or string physical type.
    Primitive { predicate: u8, physical: u8 },
    /// `StructMatchEquality<OP>`: a non-union STRUCT equality.
    StructEquality { predicate: u8, children: Vec<MatchNode> },
    /// `GenericNestedMatch<OP>`: the LIST/ARRAY path and UNION equality.
    Generic { predicate: u8 },
}

/// One predicate's column and match function.
struct MatchEntry {
    col_idx: usize,
    node: MatchNode,
}

/// The Rust owner of a `RowMatcher`: the C++ object holds only a pointer to
/// this.
struct RowMatcher {
    ops: MatcherOps,
    rhs_layout: *const c_void,
    entries: Vec<MatchEntry>,
}

/// What `matcher_match` reports back: the surviving row count and the updated
/// `no_match_count`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MatchOut {
    pub match_count: usize,
    pub no_match_count: usize,
}

//===--------------------------------------------------------------------===//
// Initialization
//===--------------------------------------------------------------------===//

/// `RowMatcher::Initialize`: builds the Rust state and returns its address.
///
/// `columns` is empty for "all columns, in order", matching the C++
/// `columns_p.empty()` default.
pub fn matcher_initialize(
    ops: MatcherOps,
    no_match_sel: bool,
    rhs_layout: *const c_void,
    predicates: &[u8],
    columns: &[u64],
) -> Result<usize, String> {
    let columns: Vec<usize> = if columns.is_empty()
    {
        (0..predicates.len()).collect()
    }
    else
    {
        columns.iter().map(|&col| col as usize).collect()
    };
    if columns.len() != predicates.len()
    {
        return Err(format!(
            "RowMatcher::Initialize: {} columns for {} predicates",
            columns.len(),
            predicates.len()
        ));
    }

    let mut entries = Vec::with_capacity(predicates.len());
    for (&col_idx, &predicate) in columns.iter().zip(predicates.iter())
    {
        let ty = (ops.layout_type)(rhs_layout, col_idx);
        let node = build_node(&ops, ty, predicate, no_match_sel)?;
        entries.push(MatchEntry { col_idx, node });
    }

    let matcher = Box::new(RowMatcher { ops, rhs_layout, entries });
    Ok(Box::into_raw(matcher) as usize)
}

/// `RowMatcher::GetMatchFunction` for a logical type.
fn build_node(
    ops: &MatcherOps,
    ty: *const c_void,
    predicate: u8,
    _no_match_sel: bool,
) -> Result<MatchNode, String> {
    use expression_type as expr;
    use physical as phys;

    let physical = (ops.type_physical)(ty);
    match physical
    {
        phys::BOOL
        | phys::INT8
        | phys::INT16
        | phys::INT32
        | phys::INT64
        | phys::INT128
        | phys::UINT8
        | phys::UINT16
        | phys::UINT32
        | phys::UINT64
        | phys::UINT128
        | phys::FLOAT
        | phys::DOUBLE
        | phys::INTERVAL
        | phys::VARCHAR =>
        {
            check_primitive_predicate(predicate)?;
            Ok(MatchNode::Primitive { predicate, physical })
        },
        phys::STRUCT =>
        {
            let is_union = (ops.type_is_union)(ty);
            match predicate
            {
                expr::COMPARE_EQUAL =>
                {
                    if is_union
                    {
                        Ok(MatchNode::Generic { predicate })
                    }
                    else
                    {
                        // STRUCT equality uses DISTINCT semantics for the internal columns
                        Ok(MatchNode::StructEquality {
                            predicate,
                            children: build_struct_children(
                                ops,
                                ty,
                                expr::COMPARE_NOT_DISTINCT_FROM,
                            )?,
                        })
                    }
                },
                expr::COMPARE_NOTEQUAL
                | expr::COMPARE_DISTINCT_FROM
                | expr::COMPARE_GREATERTHAN
                | expr::COMPARE_GREATERTHANOREQUALTO
                | expr::COMPARE_LESSTHAN
                | expr::COMPARE_LESSTHANOREQUALTO => Ok(MatchNode::Generic { predicate }),
                expr::COMPARE_NOT_DISTINCT_FROM =>
                {
                    if is_union
                    {
                        Ok(MatchNode::Generic { predicate })
                    }
                    else
                    {
                        Ok(MatchNode::StructEquality {
                            predicate,
                            children: build_struct_children(ops, ty, predicate)?,
                        })
                    }
                },
                other => Err(format!(
                    "Unsupported ExpressionType for RowMatcher::GetStructMatchFunction: {other}"
                )),
            }
        },
        phys::LIST | phys::ARRAY =>
        {
            // Same logic as for lists
            check_primitive_predicate_loose(predicate)?;
            Ok(MatchNode::Generic { predicate })
        },
        other => Err(format!("Unsupported PhysicalType for RowMatcher::GetMatchFunction: {other}")),
    }
}

/// The predicate check `GetStructMatchFunction` applies before recursing.
fn check_primitive_predicate(predicate: u8) -> Result<(), String> {
    use expression_type as expr;
    match predicate
    {
        expr::COMPARE_EQUAL
        | expr::COMPARE_NOTEQUAL
        | expr::COMPARE_DISTINCT_FROM
        | expr::COMPARE_NOT_DISTINCT_FROM
        | expr::COMPARE_GREATERTHAN
        | expr::COMPARE_GREATERTHANOREQUALTO
        | expr::COMPARE_LESSTHAN
        | expr::COMPARE_LESSTHANOREQUALTO => Ok(()),
        other =>
        {
            Err(format!("Unsupported ExpressionType for RowMatcher::GetMatchFunction: {other}"))
        },
    }
}

/// `GetListMatchFunction` rejects the same predicates as the primitive path.
fn check_primitive_predicate_loose(predicate: u8) -> Result<(), String> {
    check_primitive_predicate(predicate).map_err(|_| {
        format!("Unsupported ExpressionType for RowMatcher::GetListMatchFunction: {predicate}")
    })
}

/// `GetStructMatchFunction`'s recursion into the struct's child types.
fn build_struct_children(
    ops: &MatcherOps,
    ty: *const c_void,
    child_predicate: u8,
) -> Result<Vec<MatchNode>, String> {
    let child_count = (ops.type_struct_child_count)(ty);
    let mut children = Vec::with_capacity(child_count);
    for child_idx in 0..child_count
    {
        let child_ty = (ops.type_struct_child)(ty, child_idx);
        children.push(build_node(ops, child_ty, child_predicate, false)?);
    }
    Ok(children)
}

/// `RowMatcher::~RowMatcher`'s release of the Rust state.
#[allow(unsafe_code)]
pub fn matcher_free(matcher: usize) {
    if matcher == 0
    {
        return;
    }
    let ptr = matcher as *mut RowMatcher;
    // SAFETY: `matcher` comes from `matcher_initialize`, which leaked exactly one
    // `Box<RowMatcher>`. The C++ `RowMatcher` calls this once from its destructor
    // (or from the move-assignment that replaces its pointer), so the box is
    // reclaimed exactly once and never used again.
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

//===--------------------------------------------------------------------===//
// Matching
//===--------------------------------------------------------------------===//

/// `RowMatcher::Match`.
///
/// `sel` carries the row indices to compare and is rewritten in place with the
/// survivors; `no_match_sel` is empty when the caller passed no selection. The
/// returned [`MatchOut`] carries the survivor count and the updated
/// `no_match_count`.
#[allow(clippy::too_many_arguments)]
#[allow(unsafe_code)]
pub fn matcher_match(
    matcher: usize,
    lhs: *mut c_void,
    lhs_formats: *const c_void,
    sel: &mut [u32],
    count: usize,
    rhs_row_locations: *mut c_void,
    no_match_sel: &mut [u32],
    no_match_count_in: usize,
) -> Result<MatchOut, String> {
    if matcher == 0
    {
        return Err("RowMatcher::Match called on an uninitialized matcher".to_string());
    }
    // SAFETY: `matcher` is the live pointer `matcher_initialize` returned and the
    // C++ owner keeps it alive for the duration of `Match`; the `RowMatcher` is
    // never mutated after initialisation, so a shared borrow is enough.
    let matcher = unsafe { &*(matcher as *const RowMatcher) };

    let mut count = count;
    let mut no_match_count = no_match_count_in;
    for entry in &matcher.entries
    {
        let lhs_vector = (matcher.ops.chunk_child_vector)(lhs, entry.col_idx);
        let lhs_format = (matcher.ops.lhs_format_at)(lhs_formats, entry.col_idx);
        count = match_node(
            &matcher.ops,
            &entry.node,
            lhs_vector,
            lhs_format,
            sel,
            count,
            matcher.rhs_layout,
            rhs_row_locations,
            entry.col_idx,
            no_match_sel,
            &mut no_match_count,
        );
    }
    Ok(MatchOut { match_count: count, no_match_count })
}

#[allow(clippy::too_many_arguments)]
fn match_node(
    ops: &MatcherOps,
    node: &MatchNode,
    lhs_vector: *mut c_void,
    lhs_format: *const c_void,
    sel: &mut [u32],
    count: usize,
    rhs_layout: *const c_void,
    rhs_row_locations: *mut c_void,
    col_idx: usize,
    no_match_sel: &mut [u32],
    no_match_count: &mut usize,
) -> usize {
    match node
    {
        MatchNode::Primitive { predicate, physical } => match_primitive(
            ops,
            *predicate,
            *physical,
            lhs_format,
            sel,
            count,
            rhs_layout,
            rhs_row_locations,
            col_idx,
            no_match_sel,
            no_match_count,
        ),
        MatchNode::StructEquality { predicate, children } => match_struct(
            ops,
            *predicate,
            children,
            lhs_vector,
            lhs_format,
            sel,
            count,
            rhs_layout,
            rhs_row_locations,
            col_idx,
            no_match_sel,
            no_match_count,
        ),
        MatchNode::Generic { predicate } => match_generic(
            ops,
            *predicate,
            lhs_vector,
            sel,
            count,
            rhs_layout,
            rhs_row_locations,
            col_idx,
            no_match_sel,
            no_match_count,
        ),
    }
}

/// `TemplatedMatchLoop`.
#[allow(clippy::too_many_arguments)]
fn match_primitive(
    ops: &MatcherOps,
    predicate: u8,
    physical: u8,
    lhs_format: *const c_void,
    sel: &mut [u32],
    count: usize,
    rhs_layout: *const c_void,
    rhs_row_locations: *mut c_void,
    col_idx: usize,
    no_match_sel: &mut [u32],
    no_match_count: &mut usize,
) -> usize {
    let lhs_all_valid = (ops.lhs_cannot_have_null)(lhs_format);
    let rhs_all_valid = !(ops.layout_can_have_null)(rhs_layout);
    let rhs_offset = (ops.layout_offset)(rhs_layout, col_idx);
    let rhs_column_count = (ops.layout_column_count)(rhs_layout);
    let want_no_match = !no_match_sel.is_empty();

    let mut match_count = 0usize;
    for row in 0..count
    {
        let idx = sel[row];
        let location = (ops.rhs_location_at)(rhs_row_locations, idx as usize);
        let rhs_null = if rhs_all_valid
        {
            false
        }
        else
        {
            (ops.rhs_row_is_null)(rhs_layout, location, col_idx, rhs_column_count)
        };
        let matched = (ops.compare_row)(
            lhs_format,
            idx as usize,
            predicate,
            physical,
            location + rhs_offset,
            rhs_null,
            lhs_all_valid,
        );
        if matched
        {
            sel[match_count] = idx;
            match_count += 1;
        }
        else if want_no_match
        {
            no_match_sel[*no_match_count] = idx;
            *no_match_count += 1;
        }
    }
    match_count
}

/// `StructMatchEquality`.
#[allow(clippy::too_many_arguments)]
fn match_struct(
    ops: &MatcherOps,
    predicate: u8,
    children: &[MatchNode],
    lhs_vector: *mut c_void,
    lhs_format: *const c_void,
    sel: &mut [u32],
    count: usize,
    rhs_layout: *const c_void,
    rhs_row_locations: *mut c_void,
    col_idx: usize,
    no_match_sel: &mut [u32],
    no_match_count: &mut usize,
) -> usize {
    let lhs_all_valid = (ops.lhs_cannot_have_null)(lhs_format);
    let rhs_can_have_null = (ops.layout_can_have_null)(rhs_layout);
    let rhs_column_count = (ops.layout_column_count)(rhs_layout);
    let rhs_offset = (ops.layout_offset)(rhs_layout, col_idx);
    let want_no_match = !no_match_sel.is_empty();

    let mut match_count = 0usize;
    for row in 0..count
    {
        let idx = sel[row];
        let lhs_null = if lhs_all_valid
        {
            false
        }
        else
        {
            (ops.lhs_row_is_null)(lhs_format, idx as usize)
        };
        let location = (ops.rhs_location_at)(rhs_row_locations, idx as usize);
        // For structs there is no value to compare: match NULLs and let the recursion
        // do the rest
        let rhs_null = rhs_can_have_null
            && (ops.rhs_row_is_null)(rhs_layout, location, col_idx, rhs_column_count);
        if !(lhs_null || rhs_null) || (ops.struct_null_keep)(predicate, lhs_null, rhs_null)
        {
            sel[match_count] = idx;
            match_count += 1;
        }
        else if want_no_match
        {
            no_match_sel[*no_match_count] = idx;
            *no_match_count += 1;
        }
    }

    // A Vector of pointers to the start of each matched STRUCT in its row
    let struct_row_locations =
        (ops.build_struct_row_locations)(rhs_row_locations, sel.as_ptr(), match_count, rhs_offset);
    if struct_row_locations.is_null()
    {
        // The trampoline caught a C++ exception; the boundary rethrows it.
        return match_count;
    }
    let rhs_struct_layout = (ops.layout_struct)(rhs_layout, col_idx);
    let lhs_struct_entries = (ops.struct_entries)(lhs_vector);

    for (struct_col_idx, child) in children.iter().enumerate()
    {
        let lhs_struct_vector = (ops.struct_entry)(lhs_struct_entries, struct_col_idx);
        let lhs_struct_format = (ops.lhs_format_child)(lhs_format, struct_col_idx);
        match_count = match_node(
            ops,
            child,
            lhs_struct_vector,
            lhs_struct_format,
            sel,
            match_count,
            rhs_struct_layout,
            struct_row_locations,
            struct_col_idx,
            no_match_sel,
            no_match_count,
        );
    }

    (ops.free_vector)(struct_row_locations);
    match_count
}

/// `GenericNestedMatch`.
#[allow(clippy::too_many_arguments)]
fn match_generic(
    ops: &MatcherOps,
    predicate: u8,
    lhs_vector: *mut c_void,
    sel: &mut [u32],
    count: usize,
    rhs_layout: *const c_void,
    rhs_row_locations: *mut c_void,
    col_idx: usize,
    no_match_sel: &mut [u32],
    no_match_count: &mut usize,
) -> usize {
    let (no_match_ptr, no_match_capacity) = if no_match_sel.is_empty()
    {
        (std::ptr::null_mut(), 0usize)
    }
    else
    {
        (no_match_sel[*no_match_count..].as_mut_ptr(), no_match_sel.len() - *no_match_count)
    };
    (ops.generic_nested_match)(
        predicate,
        rhs_layout,
        col_idx,
        lhs_vector,
        rhs_row_locations,
        sel.as_mut_ptr(),
        count,
        no_match_ptr,
        no_match_capacity,
        no_match_count,
    )
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    // A fake type tree: type handles are `index + 1` as pointers.
    thread_local! {
        static TYPES: RefCell<Vec<FakeType>> = const { RefCell::new(Vec::new()) };
    }

    #[derive(Clone)]
    struct FakeType {
        physical: u8,
        is_union: bool,
        children: Vec<usize>,
    }

    fn set_types(types: Vec<FakeType>) { TYPES.with(|t| *t.borrow_mut() = types); }

    extern "C" fn fake_layout_column_count(_layout: *const c_void) -> usize { 0 }
    extern "C" fn fake_layout_can_have_null(_layout: *const c_void) -> bool { false }
    extern "C" fn fake_layout_offset(_layout: *const c_void, _col: usize) -> usize { 0 }
    extern "C" fn fake_layout_type(_layout: *const c_void, col: usize) -> *const c_void {
        (col + 1) as *const c_void
    }
    extern "C" fn fake_layout_struct(_layout: *const c_void, _col: usize) -> *const c_void {
        std::ptr::null()
    }
    extern "C" fn fake_type_physical(ty: *const c_void) -> u8 {
        let idx = ty as usize - 1;
        TYPES.with(|t| t.borrow()[idx].physical)
    }
    extern "C" fn fake_type_is_union(ty: *const c_void) -> bool {
        let idx = ty as usize - 1;
        TYPES.with(|t| t.borrow()[idx].is_union)
    }
    extern "C" fn fake_type_struct_child_count(ty: *const c_void) -> usize {
        let idx = ty as usize - 1;
        TYPES.with(|t| t.borrow()[idx].children.len())
    }
    extern "C" fn fake_type_struct_child(ty: *const c_void, child: usize) -> *const c_void {
        let idx = ty as usize - 1;
        let child_idx = TYPES.with(|t| t.borrow()[idx].children[child]);
        (child_idx + 1) as *const c_void
    }
    extern "C" fn fake_lhs_format_at(_formats: *const c_void, _col: usize) -> *const c_void {
        std::ptr::null()
    }
    extern "C" fn fake_lhs_format_child(_format: *const c_void, _col: usize) -> *const c_void {
        std::ptr::null()
    }
    extern "C" fn fake_lhs_cannot_have_null(_format: *const c_void) -> bool { true }
    extern "C" fn fake_lhs_row_is_null(_format: *const c_void, _idx: usize) -> bool { false }
    extern "C" fn fake_compare_row(
        _format: *const c_void,
        _idx: usize,
        _predicate: u8,
        _physical: u8,
        _rhs_ptr: usize,
        _rhs_null: bool,
        _lhs_all_valid: bool,
    ) -> bool {
        false
    }
    extern "C" fn fake_rhs_row_is_null(
        _layout: *const c_void,
        _location: usize,
        _col: usize,
        _count: usize,
    ) -> bool {
        false
    }
    extern "C" fn fake_rhs_location_at(_locations: *mut c_void, _idx: usize) -> usize { 0 }
    extern "C" fn fake_struct_null_keep(_predicate: u8, _lhs_null: bool, _rhs_null: bool) -> bool {
        false
    }
    extern "C" fn fake_build_struct_row_locations(
        _locations: *mut c_void,
        _sel: *const u32,
        _match_count: usize,
        _offset: usize,
    ) -> *mut c_void {
        std::ptr::null_mut()
    }
    extern "C" fn fake_free_vector(_vector: *mut c_void) {}
    extern "C" fn fake_struct_entries(_lhs_vector: *mut c_void) -> *mut c_void {
        std::ptr::null_mut()
    }
    extern "C" fn fake_struct_entry(_entries: *mut c_void, _idx: usize) -> *mut c_void {
        std::ptr::null_mut()
    }
    extern "C" fn fake_generic_nested_match(
        _predicate: u8,
        _layout: *const c_void,
        _col: usize,
        _lhs_vector: *mut c_void,
        _locations: *mut c_void,
        _sel: *mut u32,
        _count: usize,
        _no_match_sel: *mut u32,
        _no_match_capacity: usize,
        _no_match_count: *mut usize,
    ) -> usize {
        0
    }
    extern "C" fn fake_chunk_child_vector(_chunk: *mut c_void, _col: usize) -> *mut c_void {
        std::ptr::null_mut()
    }

    fn ops() -> MatcherOps {
        MatcherOps::new_(
            fake_layout_column_count,
            fake_layout_can_have_null,
            fake_layout_offset,
            fake_layout_type,
            fake_layout_struct,
            fake_type_physical,
            fake_type_is_union,
            fake_type_struct_child_count,
            fake_type_struct_child,
            fake_lhs_format_at,
            fake_lhs_format_child,
            fake_lhs_cannot_have_null,
            fake_lhs_row_is_null,
            fake_compare_row,
            fake_rhs_row_is_null,
            fake_rhs_location_at,
            fake_struct_null_keep,
            fake_build_struct_row_locations,
            fake_free_vector,
            fake_struct_entries,
            fake_struct_entry,
            fake_generic_nested_match,
            fake_chunk_child_vector,
        )
    }

    fn handle(index: usize) -> *const c_void { (index + 1) as *const c_void }

    fn assert_primitive(node: &MatchNode, predicate: u8, physical: u8) {
        match node
        {
            MatchNode::Primitive { predicate: p, physical: ph } =>
            {
                assert_eq!((*p, *ph), (predicate, physical));
            },
            _ => panic!("expected a primitive node"),
        }
    }

    #[test]
    fn every_fixed_width_type_is_primitive() {
        use physical as phys;
        let physicals = [
            phys::BOOL,
            phys::INT8,
            phys::INT16,
            phys::INT32,
            phys::INT64,
            phys::INT128,
            phys::UINT8,
            phys::UINT16,
            phys::UINT32,
            phys::UINT64,
            phys::UINT128,
            phys::FLOAT,
            phys::DOUBLE,
            phys::INTERVAL,
            phys::VARCHAR,
        ];
        set_types(
            physicals
                .iter()
                .map(|&physical| FakeType { physical, is_union: false, children: vec![] })
                .collect(),
        );
        let ops = ops();
        for (index, &physical) in physicals.iter().enumerate()
        {
            let node =
                build_node(&ops, handle(index), expression_type::COMPARE_EQUAL, false).unwrap();
            assert_primitive(&node, expression_type::COMPARE_EQUAL, physical);
        }
    }

    #[test]
    fn list_and_array_are_generic() {
        use physical as phys;
        set_types(vec![
            FakeType { physical: phys::LIST, is_union: false, children: vec![] },
            FakeType { physical: phys::ARRAY, is_union: false, children: vec![] },
        ]);
        let ops = ops();
        assert!(matches!(
            build_node(&ops, handle(0), expression_type::COMPARE_EQUAL, false).unwrap(),
            MatchNode::Generic { .. }
        ));
        assert!(matches!(
            build_node(&ops, handle(1), expression_type::COMPARE_DISTINCT_FROM, false).unwrap(),
            MatchNode::Generic { .. }
        ));
    }

    #[test]
    fn struct_equality_recurses_with_not_distinct_semantics() {
        use physical as phys;
        // A STRUCT of INT32 and INT64.
        set_types(vec![
            FakeType { physical: phys::STRUCT, is_union: false, children: vec![1, 2] },
            FakeType { physical: phys::INT32, is_union: false, children: vec![] },
            FakeType { physical: phys::INT64, is_union: false, children: vec![] },
        ]);
        let ops = ops();
        let node = build_node(&ops, handle(0), expression_type::COMPARE_EQUAL, false).unwrap();
        match node
        {
            MatchNode::StructEquality { predicate, children } =>
            {
                assert_eq!(predicate, expression_type::COMPARE_EQUAL);
                assert_eq!(children.len(), 2);
                assert_primitive(
                    &children[0],
                    expression_type::COMPARE_NOT_DISTINCT_FROM,
                    phys::INT32,
                );
                assert_primitive(
                    &children[1],
                    expression_type::COMPARE_NOT_DISTINCT_FROM,
                    phys::INT64,
                );
            },
            _ => panic!("expected a struct-equality node"),
        }
    }

    #[test]
    fn union_equality_is_generic() {
        use physical as phys;
        set_types(vec![
            FakeType { physical: phys::STRUCT, is_union: true, children: vec![1] },
            FakeType { physical: phys::INT32, is_union: false, children: vec![] },
        ]);
        let ops = ops();
        assert!(matches!(
            build_node(&ops, handle(0), expression_type::COMPARE_EQUAL, false).unwrap(),
            MatchNode::Generic { .. }
        ));
    }

    #[test]
    fn unsupported_physical_type_is_an_error() {
        set_types(vec![FakeType { physical: 250, is_union: false, children: vec![] }]);
        let ops = ops();
        assert!(build_node(&ops, handle(0), expression_type::COMPARE_EQUAL, false).is_err());
    }

    #[test]
    fn initialize_defaults_to_all_columns() {
        use physical as phys;
        set_types(vec![
            FakeType { physical: phys::INT32, is_union: false, children: vec![] },
            FakeType { physical: phys::INT64, is_union: false, children: vec![] },
        ]);
        let ops = ops();
        let matcher = matcher_initialize(ops, false, std::ptr::null(), &[25, 25], &[]).unwrap();
        {
            let m = unsafe { &*(matcher as *const RowMatcher) };
            assert_eq!(m.entries.len(), 2);
            assert_eq!(m.entries[0].col_idx, 0);
            assert_eq!(m.entries[1].col_idx, 1);
        }
        matcher_free(matcher);
    }

    #[test]
    fn initialize_rejects_a_column_count_mismatch() {
        set_types(vec![FakeType { physical: 7, is_union: false, children: vec![] }]);
        let ops = ops();
        assert!(matcher_initialize(ops, false, std::ptr::null(), &[25, 25], &[0]).is_err());
    }

    // --- a full primitive match over a fabricated layout ---------------------

    thread_local! {
        static LHS_DATA: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
        static RHS_DATA: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
        static LHS_INDEX: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        static LOCATIONS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn match_layout_column_count(_layout: *const c_void) -> usize { 1 }
    extern "C" fn match_layout_offset(_layout: *const c_void, _col: usize) -> usize { 0 }
    extern "C" fn match_compare_row(
        _format: *const c_void,
        idx: usize,
        predicate: u8,
        _physical: u8,
        rhs_ptr: usize,
        rhs_null: bool,
        _lhs_all_valid: bool,
    ) -> bool {
        if rhs_null
        {
            return false;
        }
        let lhs_idx = LHS_INDEX.with(|s| s.borrow()[idx] as usize);
        let lhs = LHS_DATA.with(|d| d.borrow()[lhs_idx]);
        let rhs = unsafe { *(rhs_ptr as *const i32) };
        match predicate
        {
            expression_type::COMPARE_EQUAL => lhs == rhs,
            _ => panic!("unexpected predicate"),
        }
    }
    extern "C" fn match_rhs_location_at(_locations: *mut c_void, idx: usize) -> usize {
        let rhs_idx = LOCATIONS.with(|l| l.borrow()[idx] as usize);
        RHS_DATA.with(|d| d.borrow().as_ptr().wrapping_add(rhs_idx) as usize)
    }

    fn match_ops() -> MatcherOps {
        MatcherOps::new_(
            match_layout_column_count,
            fake_layout_can_have_null,
            match_layout_offset,
            fake_layout_type,
            fake_layout_struct,
            fake_type_physical,
            fake_type_is_union,
            fake_type_struct_child_count,
            fake_type_struct_child,
            fake_lhs_format_at,
            fake_lhs_format_child,
            fake_lhs_cannot_have_null,
            fake_lhs_row_is_null,
            match_compare_row,
            fake_rhs_row_is_null,
            match_rhs_location_at,
            fake_struct_null_keep,
            fake_build_struct_row_locations,
            fake_free_vector,
            fake_struct_entries,
            fake_struct_entry,
            fake_generic_nested_match,
            fake_chunk_child_vector,
        )
    }

    #[test]
    fn primitive_match_keeps_only_the_equal_rows() {
        use physical as phys;
        // LHS values 10, 20, 30 (read through the identity selection); RHS rows
        // 20, 20, 30 -> the rows at index 1 and 2 match.
        set_types(vec![FakeType { physical: phys::INT32, is_union: false, children: vec![] }]);
        let mut lhs = [10i32, 20, 30];
        let mut rhs = [20i32, 20, 30];
        let mut locs = [0u64, 1, 2];
        let mut index: Vec<u32> = (0..3).collect();
        LHS_DATA.with(|d| *d.borrow_mut() = lhs.to_vec());
        RHS_DATA.with(|d| *d.borrow_mut() = rhs.to_vec());
        LOCATIONS.with(|l| *l.borrow_mut() = locs.to_vec());
        LHS_INDEX.with(|s| *s.borrow_mut() = std::mem::take(&mut index));

        let ops = match_ops();
        let matcher = matcher_initialize(
            ops,
            false,
            std::ptr::null(),
            &[expression_type::COMPARE_EQUAL],
            &[],
        )
        .unwrap();
        let mut sel = [0u32, 1, 2];
        let mut no_match = [0u32; 3];
        let out = matcher_match(
            matcher,
            std::ptr::null_mut(),
            std::ptr::null(),
            &mut sel,
            3,
            std::ptr::null_mut(),
            &mut no_match,
            0,
        )
        .unwrap();
        matcher_free(matcher);
        assert_eq!(out.match_count, 2);
        assert_eq!(sel[0], 1);
        assert_eq!(sel[1], 2);
        assert_eq!(no_match[0], 0);
        // Silence the unused-mut warnings of the local test buffers.
        let _ = (&mut lhs, &mut rhs, &mut locs);
    }
}
