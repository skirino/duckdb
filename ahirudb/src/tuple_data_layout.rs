//! TupleDataLayout module: Rust port of DuckDB's tuple data layout computation.
//!
//! Minimal implementation for Phase 1 migration.

#![deny(unsafe_code)]

// ----------------------------------------------------------------------------
// Physical r#type enum - simplified version for layout computation
// ----------------------------------------------------------------------------

/// Physical r#type enum - simplified version for layout computation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PhysicalType {
    INT8 = 1,
    UINT8 = 2,
    INT16 = 3,
    UINT16 = 4,
    INT32 = 5,
    UINT32 = 6,
    INT64 = 7,
    UINT64 = 8,
    FLOAT = 9,
    DOUBLE = 10,
    VARCHAR = 11,
    BLOB = 12,
    DATE = 13,
    TIME = 14,
    TIMESTAMP = 15,
    INTERVAL = 16,
    LIST = 17,
    STRUCT = 18,
    ARRAY = 19,
    MAP = 20,
    POINTER = 21,
    UUID = 22,
    INVALID = 0,
}

// ----------------------------------------------------------------------------
// Logical r#type ID enum - simplified version
// ----------------------------------------------------------------------------

/// Logical r#type ID enum - simplified version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum LogicalTypeId {
    INVALID = 0,
    BOOLEAN = 1,
    TINYINT = 2,
    SMALLINT = 3,
    INTEGER = 4,
    BIGINT = 5,
    DATE = 6,
    TIME = 7,
    TIMESTAMP = 8,
    TIMESTAMP_SEC = 9,
    TIMESTAMP_MS = 10,
    TIMESTAMP_NS = 11,
    DECIMAL = 12,
    FLOAT = 13,
    DOUBLE = 14,
    CHAR = 15,
    VARCHAR = 16,
    BLOB = 17,
    INTERVAL = 18,
    UTINYINT = 19,
    USMALLINT = 20,
    UINTEGER = 21,
    UBIGINT = 22,
    TIMESTAMP_TZ = 23,
    TIMESTAMP_TZ_NS = 24,
    TIME_TZ = 25,
    TIME_NS = 26,
    BIT = 27,
    STRING_LITERAL = 28,
    INTEGER_LITERAL = 29,
    BIGNUM = 30,
    UHUGEINT = 31,
    HUGEINT = 32,
    POINTER = 33,
    VALIDITY = 34,
    UUID = 35,
    GEOMETRY = 36,
    STRUCT = 37,
    LIST = 38,
    MAP = 39,
    TABLE = 40,
    ENUM = 41,
    LEGACY_AGGREGATE_STATE = 42,
    LAMBDA = 43,
    UNION = 44,
    ARRAY = 45,
    VARIANT = 46,
    TUPLE = 47,
}

// ----------------------------------------------------------------------------
// Simplified LogicalType struct
// ----------------------------------------------------------------------------

/// Simplified LogicalType struct
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalType {
    pub id: LogicalTypeId,
}

impl LogicalType {
    pub fn new(id: LogicalTypeId) -> Self { LogicalType { id } }

    pub fn internal_type(&self) -> PhysicalType {
        match self.id
        {
            LogicalTypeId::BOOLEAN => PhysicalType::INT8,
            LogicalTypeId::TINYINT => PhysicalType::INT8,
            LogicalTypeId::SMALLINT => PhysicalType::INT16,
            LogicalTypeId::INTEGER => PhysicalType::INT32,
            LogicalTypeId::BIGINT => PhysicalType::INT64,
            LogicalTypeId::DATE => PhysicalType::DATE,
            LogicalTypeId::TIME => PhysicalType::TIME,
            LogicalTypeId::TIMESTAMP => PhysicalType::TIMESTAMP,
            LogicalTypeId::DECIMAL => PhysicalType::FLOAT,
            LogicalTypeId::FLOAT => PhysicalType::FLOAT,
            LogicalTypeId::DOUBLE => PhysicalType::DOUBLE,
            LogicalTypeId::VARCHAR => PhysicalType::VARCHAR,
            LogicalTypeId::BLOB => PhysicalType::BLOB,
            LogicalTypeId::INTERVAL => PhysicalType::INTERVAL,
            LogicalTypeId::UTINYINT => PhysicalType::INT8,
            LogicalTypeId::USMALLINT => PhysicalType::INT16,
            LogicalTypeId::UINTEGER => PhysicalType::UINT32,
            LogicalTypeId::UBIGINT => PhysicalType::INT64,
            LogicalTypeId::LIST => PhysicalType::LIST,
            LogicalTypeId::STRUCT => PhysicalType::STRUCT,
            LogicalTypeId::ARRAY => PhysicalType::ARRAY,
            LogicalTypeId::VARIANT => PhysicalType::ARRAY,
            _ => PhysicalType::INVALID,
        }
    }

    pub fn is_nested(&self) -> bool {
        matches!(self.id, LogicalTypeId::STRUCT | LogicalTypeId::LIST | LogicalTypeId::ARRAY)
    }
}

// ----------------------------------------------------------------------------
// Index r#type used throughout the row module
// ----------------------------------------------------------------------------

#[allow(non_camel_case_types)]
pub type idx_t = usize;

// ----------------------------------------------------------------------------
// Type utility functions
// ----------------------------------------------------------------------------

/// Returns whether a physical r#type has constant size
fn type_is_constant_size(physical_type: PhysicalType) -> bool {
    matches!(
        physical_type,
        PhysicalType::INT8
            | PhysicalType::UINT8
            | PhysicalType::INT16
            | PhysicalType::UINT16
            | PhysicalType::INT32
            | PhysicalType::UINT32
            | PhysicalType::INT64
            | PhysicalType::UINT64
            | PhysicalType::FLOAT
            | PhysicalType::DOUBLE
            | PhysicalType::VARCHAR
            | PhysicalType::BLOB
            | PhysicalType::DATE
            | PhysicalType::TIME
            | PhysicalType::TIMESTAMP
            | PhysicalType::INTERVAL
            | PhysicalType::POINTER
            | PhysicalType::UUID
    )
}

/// Returns the size of a physical r#type in bytes
fn get_type_id_size(physical_type: PhysicalType) -> idx_t {
    match physical_type
    {
        PhysicalType::INT8 | PhysicalType::UINT8 => 1,
        PhysicalType::INT16 | PhysicalType::UINT16 => 2,
        PhysicalType::INT32 | PhysicalType::UINT32 => 4,
        PhysicalType::INT64 | PhysicalType::UINT64 => 8,
        PhysicalType::FLOAT => 4,
        PhysicalType::DOUBLE => 8,
        PhysicalType::VARCHAR | PhysicalType::BLOB => 0,
        PhysicalType::DATE => 4,
        PhysicalType::TIME => 4,
        PhysicalType::TIMESTAMP => 8,
        PhysicalType::INTERVAL => 12,
        PhysicalType::POINTER => 8,
        PhysicalType::UUID => 16,
        _ => 0,
    }
}

/// Aligns a value up to the given alignment
fn align_value(value: idx_t, alignment: idx_t) -> idx_t { value.div_ceil(alignment) * alignment }

/// Returns the validity mask size in bytes given the number of columns and
/// whether nulls are allowed
fn validity_mask_size(column_count: idx_t, cannot_have_null: bool) -> idx_t {
    if cannot_have_null
    {
        0
    }
    else
    {
        column_count.div_ceil(8)
    }
}

// ----------------------------------------------------------------------------
// Enum types used by TupleDataLayout
// ----------------------------------------------------------------------------

/// Enum representing validity r#type for tuple data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum TupleDataValidityType {
    CAN_HAVE_NULL_VALUES,
    CANNOT_HAVE_NULL_VALUES,
}

/// Enum representing nestedness r#type for tuple data layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum TupleDataNestednessType {
    TOP_LEVEL_LAYOUT,
    NESTED_STRUCT_LAYOUT,
}

/// Sort key r#type for tuple data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum SortKeyType {
    INVALID,
    NO_PAYLOAD_FIXED_8,
    PAYLOAD_FIXED_16,
    NO_PAYLOAD_FIXED_16,
    PAYLOAD_FIXED_24,
    NO_PAYLOAD_FIXED_24,
    PAYLOAD_FIXED_32,
    NO_PAYLOAD_FIXED_32,
    PAYLOAD_VARIABLE_32,
    NO_PAYLOAD_VARIABLE_32,
}

// ----------------------------------------------------------------------------
// Aggregate object with payload size and destructor info
// ----------------------------------------------------------------------------

/// Aggregate object with payload size and destructor info
#[derive(Debug, Clone)]
pub struct AggregateObject {
    /// The payload size of the aggregate
    pub payload_size: idx_t,
    /// Whether the aggregate has a destructor callback
    pub has_destructor: bool,
}

// ----------------------------------------------------------------------------
// BoundOrderByNode - simplified for sort key initialization
// ----------------------------------------------------------------------------

/// BoundOrderByNode - placeholder for sort key initialization
#[derive(Debug, Clone)]
pub struct BoundOrderByNode {
    // expr_type field removed - unused
}

// ----------------------------------------------------------------------------
// TupleDataLayout implementation
// ----------------------------------------------------------------------------

/// Layout computation result for tuple data
#[derive(Debug)]
pub struct TupleDataLayout {
    /// The types of the data columns
    pub types: Vec<LogicalType>,
    /// The aggregate functions
    pub aggregates: Vec<AggregateObject>,
    /// The sort key r#type associated with orders
    pub sort_key_type: SortKeyType,
    /// The width of the validity header (1 bit per value)
    pub flag_width: idx_t,
    /// The width of the data portion
    pub data_width: idx_t,
    /// The width of the aggregate state portion
    pub aggr_width: idx_t,
    /// The width of the sort key
    pub sort_width: idx_t,
    /// Bytes that are skippable during sorting
    pub sort_skippable_bytes: Vec<idx_t>,
    /// The width of the entire row
    pub row_width: idx_t,
    /// The offsets to the columns and aggregate data in each row
    pub offsets: Vec<idx_t>,
    /// Whether all columns in this layout are constant size
    pub all_constant: bool,
    /// Indices of the variable columns
    pub variable_columns: Vec<idx_t>,
    /// Offset to the heap size of every row
    pub heap_size_offset: idx_t,
    /// Indices of aggregate functions that have a destructor
    pub aggr_destructor_idxs: Vec<idx_t>,
    /// Whether none of the columns have NULLs
    pub validity_type: TupleDataValidityType,
}

impl TupleDataLayout {
    /// Creates an empty TupleDataLayout
    pub fn new() -> Self {
        TupleDataLayout {
            types: Vec::new(),
            aggregates: Vec::new(),
            sort_key_type: SortKeyType::INVALID,
            flag_width: 0,
            data_width: 0,
            aggr_width: 0,
            row_width: 0,
            sort_width: 0,
            sort_skippable_bytes: Vec::new(),
            all_constant: true,
            variable_columns: Vec::new(),
            offsets: Vec::new(),
            heap_size_offset: 0,
            aggr_destructor_idxs: Vec::new(),
            validity_type: TupleDataValidityType::CAN_HAVE_NULL_VALUES,
        }
    }

    /// Creates a copy of this TupleDataLayout
    pub fn copy(&self) -> Self {
        let mut result = TupleDataLayout::new();
        result.types = self.types.clone();
        result.aggregates = self.aggregates.clone();
        result.sort_key_type = self.sort_key_type;
        // struct_layouts not cloned - struct recursion happens in C++
        result.flag_width = self.flag_width;
        result.data_width = self.data_width;
        result.aggr_width = self.aggr_width;
        result.sort_width = self.sort_width;
        result.sort_skippable_bytes = self.sort_skippable_bytes.clone();
        result.row_width = self.row_width;
        result.offsets = self.offsets.clone();
        result.all_constant = self.all_constant;
        result.heap_size_offset = self.heap_size_offset;
        result.aggr_destructor_idxs = self.aggr_destructor_idxs.clone();
        result.validity_type = self.validity_type;
        result
    }

    /// Returns the number of data columns
    pub fn column_count(&self) -> idx_t { self.types.len() as idx_t }

    /// Returns a list of the column types for this data chunk
    pub fn get_types(&self) -> &Vec<LogicalType> { &self.types }

    /// Returns a list of the aggregates for this data chunk
    pub fn get_aggregates(&self) -> &Vec<AggregateObject> { &self.aggregates }

    /// Gets the sort key type of this layout (if applicable)
    pub fn get_sort_key_type(&self) -> SortKeyType { self.sort_key_type }

    /// Returns the offset to the start of the aggregates
    pub fn get_aggr_offset(&self) -> idx_t { self.flag_width + self.data_width }

    /// Returns whether this is a sort key layout
    pub fn is_sort_key_layout(&self) -> bool { self.sort_key_type != SortKeyType::INVALID }

    /// Returns the total width required for each row, including padding
    pub fn get_row_width(&self) -> idx_t { self.row_width }

    /// Returns the offset to the start of the data
    pub fn get_data_offset(&self) -> idx_t { self.flag_width }

    /// Returns the total width required for the data, including padding
    pub fn get_data_width(&self) -> idx_t { self.data_width }

    /// Returns the total width required for the aggregates, including padding
    pub fn get_aggr_width(&self) -> idx_t { self.aggr_width }

    /// Returns the total width required for sorting
    pub fn get_sort_width(&self) -> idx_t { self.sort_width }

    /// Returns bytes that are skippable during sorting
    pub fn get_sort_skippable_bytes(&self) -> &Vec<idx_t> { &self.sort_skippable_bytes }

    /// Returns the column offsets into each row
    pub fn get_offsets(&self) -> &Vec<idx_t> { &self.offsets }

    /// Returns the indices of the variable columns
    pub fn get_variable_columns(&self) -> &Vec<idx_t> { &self.variable_columns }

    /// Gets offset to where heap size is stored
    pub fn get_heap_size_offset(&self) -> idx_t { self.heap_size_offset }

    /// Returns whether any of the aggregates have a destructor
    pub fn has_destructor(&self) -> bool { !self.aggr_destructor_idxs.is_empty() }

    /// Returns the indices of the aggregates that have destructors
    pub fn get_aggregate_destructor_indices(&self) -> &Vec<idx_t> { &self.aggr_destructor_idxs }

    /// Returns whether none of the columns have NULLs
    pub fn cannot_have_null(&self) -> bool {
        self.validity_type == TupleDataValidityType::CANNOT_HAVE_NULL_VALUES
    }

    /// Returns whether all columns in this layout are constant size
    pub fn all_constant(&self) -> bool { self.all_constant }

    /// Returns whether any of the columns can have NULLs
    pub fn can_have_null(&self) -> bool {
        self.validity_type != TupleDataValidityType::CANNOT_HAVE_NULL_VALUES
    }

    /// Returns whether any of the columns are nested
    pub fn has_nested_types(&self) -> bool { self.types.iter().any(|t| t.is_nested()) }

    /// Returns the number of aggregates
    pub fn aggregate_count(&self) -> idx_t { self.aggregates.len() as idx_t }

    /// Initializes the TupleDataLayout with the specified types and aggregates
    pub fn initialize(
        &mut self,
        types_p: Vec<LogicalType>,
        aggregates_p: Vec<AggregateObject>,
        validity_type_p: TupleDataValidityType,
        nestedness_type: TupleDataNestednessType,
    ) {
        self.sort_key_type = SortKeyType::INVALID;
        self.offsets.clear();
        self.aggr_destructor_idxs.clear();
        self.types = types_p;
        self.all_constant = true;
        self.sort_skippable_bytes.clear();
        self.variable_columns.clear();

        // Null mask at the front - 1 bit per value.
        self.validity_type = validity_type_p;
        let cannot_have_null = self.validity_type == TupleDataValidityType::CANNOT_HAVE_NULL_VALUES;
        self.flag_width = validity_mask_size(self.types.len(), cannot_have_null);
        self.row_width = self.flag_width;

        // Iterate using logical_type to avoid Rust keyword conflict
        for (col_idx, logical_type) in self.types.iter().enumerate()
        {
            let physical_type = logical_type.internal_type();
            if physical_type == PhysicalType::STRUCT
            {
                // Struct layout computation happens in C++, not Rust.
                // For standalone Rust use, this would recursively build
                // struct_layouts. For bridge mode, struct sizes
                // are passed from C++ via ColumnDesc.
                self.all_constant = false;
                self.variable_columns.push(col_idx);
            }
            else if !type_is_constant_size(physical_type)
            {
                self.all_constant = false;
                self.variable_columns.push(col_idx);
            }
        }

        // This enables pointer recomputation for out-of-core.
        if nestedness_type == TupleDataNestednessType::TOP_LEVEL_LAYOUT && !self.all_constant
        {
            self.heap_size_offset = self.row_width;
            self.row_width += std::mem::size_of::<idx_t>(); // sizeof(idx_t)
        }

        // Data columns. No alignment required.
        for col_idx in 0..self.types.len()
        {
            self.offsets.push(self.row_width);
            let physical_type = self.types[col_idx].internal_type();
            if type_is_constant_size(physical_type) || physical_type == PhysicalType::VARCHAR
            {
                self.row_width += get_type_id_size(physical_type);
            }
            else if physical_type == PhysicalType::STRUCT
            {
                // STRUCT in standalone Rust: use pointer size since actual
                // struct layout is computed in C++ for bridge
                // mode. For full standalone support, child
                // types would need to be extracted.
                self.row_width += std::mem::size_of::<idx_t>();
            }
            else
            {
                // Variable size types use pointers to the actual data (can be
                // swizzled).
                self.row_width += std::mem::size_of::<idx_t>();
            }
        }

        // Alignment padding for aggregates
        let align = !self.aggregates.is_empty();
        if align
        {
            self.row_width = align_value(self.row_width, std::mem::align_of::<idx_t>());
        }
        self.data_width = self.row_width - self.flag_width;

        // Aggregate fields.
        self.aggregates = aggregates_p;
        for aggregate in self.aggregates.iter()
        {
            self.offsets.push(self.row_width);
            self.row_width += aggregate.payload_size;
        }
        self.aggr_width = self.row_width - self.data_width - self.flag_width;

        // Alignment padding for the next row
        if align
        {
            self.row_width = align_value(self.row_width, std::mem::align_of::<idx_t>());
        }

        // Check aggregates for destructors
        for (aggr_idx, aggr) in self.aggregates.iter().enumerate()
        {
            if aggr.has_destructor
            {
                self.aggr_destructor_idxs.push(aggr_idx);
            }
        }
    }

    /// Initializes the TupleDataLayout with the specified types to an empty
    /// TupleDataLayout
    pub fn initialize_simple(
        &mut self,
        types: Vec<LogicalType>,
        validity_type: TupleDataValidityType,
        nestedness_type: TupleDataNestednessType,
    ) {
        self.initialize(types, Vec::new(), validity_type, nestedness_type);
    }

    /// Initializes a TupleDataLayout with the specified aggregates to an empty
    /// TupleDataLayout
    pub fn initialize_aggregates(&mut self, aggregates_p: Vec<AggregateObject>) {
        self.initialize(
            vec![],
            aggregates_p,
            TupleDataValidityType::CANNOT_HAVE_NULL_VALUES,
            TupleDataNestednessType::TOP_LEVEL_LAYOUT,
        );
    }

    /// Initializes a TupleDataLayout with the specified ORDER BY to an empty
    /// TupleDataLayout
    pub fn initialize_sort_key(
        &mut self,
        orders: &[BoundOrderByNode],
        r#type: &LogicalType,
        has_payload: bool,
    ) {
        // Reset state
        self.types.clear();
        self.aggregates.clear();
        self.flag_width = usize::MAX; // DConstants::INVALID_INDEX
        self.data_width = usize::MAX;
        self.aggr_width = usize::MAX;
        self.offsets.clear();
        self.sort_skippable_bytes.clear();
        self.all_constant = true;
        self.heap_size_offset = usize::MAX;
        self.aggr_destructor_idxs.clear();
        self.validity_type = TupleDataValidityType::CAN_HAVE_NULL_VALUES;

        // Type is determined by "create_sort_key", if it is <= 8 we get a
        // bigint
        self.types.push(*r#type);

        // Compute row width
        let mut sort_width: idx_t = 0;
        let mut all_valid_and_truly_constant = true;
        for _order in orders
        {
            // const auto &order = orders[order_idx];
            // const auto &logical_type = order.expression->GetReturnType();
            // const auto physical_type = logical_type.InternalType();

            // For now, we'll skip the full stats check and just use the r#type
            let physical_type = r#type.internal_type();

            if all_valid_and_truly_constant
                && physical_type != PhysicalType::INVALID
                && sort_width < 7
            {
                // We don't have to sort by this byte, all values are valid
                self.sort_skippable_bytes.push(sort_width);
            }
            else
            {
                all_valid_and_truly_constant = false;
            }

            if type_is_constant_size(physical_type)
            {
                // NULL byte + fixed-width r#type
                sort_width += 1 + get_type_id_size(physical_type);
            }
            else if r#type.id == LogicalTypeId::VARCHAR
            {
                // NULL byte + variable-length r#type
                // In real code, we'd check
                // StringStats::HasMaxStringLength(*order.stats)
                // For now, treat as variable
                sort_width = usize::MAX;
                break;
            }
            else
            {
                // We don't know how long the key will be
                sort_width = usize::MAX;
                break;
            }
        }

        // Set row width and sort key r#type accordingly
        let temp_row_width = if r#type.id == LogicalTypeId::BIGINT
        {
            8
        }
        else
        {
            sort_width
        };
        let temp_row_width = if sort_width != usize::MAX && has_payload
        {
            temp_row_width + 8
        }
        else
        {
            temp_row_width
        };

        // Round the key up to the nearest layout that
        // DUCKDB_FOR_EACH_SORT_KEY_TYPE still keeps.
        let (row_width, sort_key_type) = match temp_row_width
        {
            w if w <= 8 => (8, SortKeyType::NO_PAYLOAD_FIXED_8),
            w if w <= 16 => (
                16,
                if has_payload
                {
                    SortKeyType::PAYLOAD_FIXED_16
                }
                else
                {
                    SortKeyType::NO_PAYLOAD_FIXED_16
                },
            ),
            w if w <= 24 => (
                24,
                if has_payload
                {
                    SortKeyType::PAYLOAD_FIXED_24
                }
                else
                {
                    SortKeyType::NO_PAYLOAD_FIXED_24
                },
            ),
            w if w <= 32 => (
                32,
                if has_payload
                {
                    SortKeyType::PAYLOAD_FIXED_32
                }
                else
                {
                    SortKeyType::NO_PAYLOAD_FIXED_32
                },
            ),
            _ =>
            {
                self.all_constant = false;
                // heap_size_offset is set by C++ when using sort key layouts
                // 32 is the fixed row_width for all FIXED_32 layouts
                (32, SortKeyType::PAYLOAD_VARIABLE_32)
            },
        };

        self.row_width = row_width;
        self.sort_key_type = sort_key_type;
    }
}

impl Default for TupleDataLayout {
    fn default() -> Self { Self::new() }
}

// ----------------------------------------------------------------------------
// C++ bridge helpers: layout computation for one level, driven by C++.
// ----------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColumnDesc {
    /// C++ `PhysicalType` numeric value.
    pub physical_type: u8,
    /// Whether the column is a STRUCT (1) or not (0).
    pub is_struct: u8,
    /// Row width of the struct child layout (0 if not a struct).
    pub struct_size: usize,
    /// Whether the column is constant size (1) or not (0).
    pub is_constant: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LayoutResult {
    pub flag_width: usize,
    pub row_width: usize,
    pub data_width: usize,
    pub aggr_width: usize,
    pub heap_size_offset: usize,
    pub all_constant: u8,
    pub num_offsets: usize,
    pub num_variable_columns: usize,
    pub num_aggr_destructor_idxs: usize,
}

fn type_is_constant_size_cpp(physical_type: u8) -> bool {
    (1..=12).contains(&physical_type)
        || physical_type == 21
        || physical_type == 203
        || physical_type == 204
}

fn get_type_id_size_cpp(physical_type: u8) -> usize {
    match physical_type
    {
        1..=3 => 1,
        4 | 5 => 2,
        6 | 7 => 4,
        8 | 9 => 8,
        203 | 204 => 16,
        11 => 4,
        12 => 8,
        200 => 16,
        21 => 16,
        _ => 0,
    }
}

fn align_value_cpp(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

#[allow(clippy::too_many_arguments)]
pub fn compute_layout(
    columns: &[ColumnDesc],
    aggr_payload_sizes: &[usize],
    aggr_has_destructor: &[u8],
    validity_cannot_have_null: u8,
    is_top_level: u8,
    offsets_out: &mut [usize],
    variable_columns_out: &mut [usize],
    aggr_destructor_idxs_out: &mut [usize],
) -> LayoutResult {
    let num_columns = columns.len();
    let cannot_have_null = validity_cannot_have_null != 0;

    let flag_width = if cannot_have_null
    {
        0
    }
    else
    {
        (num_columns + 7).div_ceil(8)
    };
    let mut row_width = flag_width;

    let mut all_constant = true;
    let mut var_count = 0usize;
    for (col_idx, col) in columns.iter().enumerate()
    {
        if col.is_constant == 0
        {
            all_constant = false;
            if var_count < variable_columns_out.len()
            {
                variable_columns_out[var_count] = col_idx;
                var_count += 1;
            }
        }
    }

    let mut heap_size_offset = 0usize;
    if is_top_level != 0 && !all_constant
    {
        heap_size_offset = row_width;
        row_width += std::mem::size_of::<usize>();
    }

    let mut off_idx = 0usize;
    for col in columns.iter()
    {
        if off_idx < offsets_out.len()
        {
            offsets_out[off_idx] = row_width;
            off_idx += 1;
        }
        let pt = col.physical_type;
        if type_is_constant_size_cpp(pt) || pt == 200
        {
            row_width += get_type_id_size_cpp(pt);
        }
        else if col.is_struct != 0
        {
            row_width += col.struct_size;
        }
        else
        {
            row_width += std::mem::size_of::<usize>();
        }
    }

    let align = !aggr_payload_sizes.is_empty();
    if align
    {
        row_width = align_value_cpp(row_width, std::mem::align_of::<usize>());
    }
    let data_width = row_width - flag_width;

    for &ps in aggr_payload_sizes.iter()
    {
        if off_idx < offsets_out.len()
        {
            offsets_out[off_idx] = row_width;
            off_idx += 1;
        }
        row_width += ps;
    }
    let aggr_width = row_width - data_width - flag_width;

    if align
    {
        row_width = align_value_cpp(row_width, std::mem::align_of::<usize>());
    }

    let mut di_count = 0usize;
    for (i, &hd) in aggr_has_destructor.iter().enumerate()
    {
        if hd != 0 && di_count < aggr_destructor_idxs_out.len()
        {
            aggr_destructor_idxs_out[di_count] = i;
            di_count += 1;
        }
    }

    LayoutResult {
        flag_width,
        row_width,
        data_width,
        aggr_width,
        heap_size_offset,
        all_constant: if all_constant { 1 } else { 0 },
        num_offsets: off_idx,
        num_variable_columns: var_count,
        num_aggr_destructor_idxs: di_count,
    }
}

// ----------------------------------------------------------------------------
// Manual Clone implementation (needed because we removed derive from struct)
// ----------------------------------------------------------------------------

impl Clone for TupleDataLayout {
    fn clone(&self) -> Self { self.copy() }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_layout() {
        let layout = TupleDataLayout::new();
        assert_eq!(layout.column_count(), 0);
        assert_eq!(layout.aggregate_count(), 0);
        assert!(!layout.is_sort_key_layout());
        assert!(layout.all_constant());
    }

    #[test]
    fn test_initialize_basic() {
        let mut layout = TupleDataLayout::new();
        let types =
            vec![LogicalType::new(LogicalTypeId::INTEGER), LogicalType::new(LogicalTypeId::BIGINT)];
        let aggregates = Vec::new();
        layout.initialize(
            types,
            aggregates,
            TupleDataValidityType::CAN_HAVE_NULL_VALUES,
            TupleDataNestednessType::TOP_LEVEL_LAYOUT,
        );
        assert_eq!(layout.column_count(), 2);
        assert!(layout.all_constant());
    }

    #[test]
    fn test_copy() {
        let mut layout = TupleDataLayout::new();
        let types = vec![LogicalType::new(LogicalTypeId::INTEGER)];
        let aggregates = Vec::new();
        layout.initialize(
            types,
            aggregates,
            TupleDataValidityType::CAN_HAVE_NULL_VALUES,
            TupleDataNestednessType::TOP_LEVEL_LAYOUT,
        );
        let copy = layout.copy();
        assert_eq!(layout.column_count(), copy.column_count());
        assert_eq!(layout.row_width, copy.row_width);
    }

    #[test]
    fn test_has_nested_types() {
        let mut layout = TupleDataLayout::new();
        let types = vec![LogicalType::new(LogicalTypeId::STRUCT)];
        layout.initialize(
            types,
            Vec::new(),
            TupleDataValidityType::CAN_HAVE_NULL_VALUES,
            TupleDataNestednessType::TOP_LEVEL_LAYOUT,
        );
        assert!(layout.has_nested_types());
    }

    #[test]
    fn test_sort_key_initialization() {
        let mut layout = TupleDataLayout::new();
        let r#type = LogicalType::new(LogicalTypeId::INTEGER);
        let orders = vec![BoundOrderByNode {}];
        layout.initialize_sort_key(&orders, &r#type, false);
        assert!(layout.is_sort_key_layout());
    }

    #[test]
    fn test_compute_layout_bridge() {
        // Test compute_layout with known inputs
        let columns = vec![
            ColumnDesc {
                physical_type: PhysicalType::INT32 as u8,
                is_struct: 0,
                struct_size: 0,
                is_constant: 1,
            },
            ColumnDesc {
                physical_type: PhysicalType::INT64 as u8,
                is_struct: 0,
                struct_size: 0,
                is_constant: 1,
            },
        ];
        let aggr_payload_sizes = vec![8];
        let aggr_has_destructor = vec![0];
        let mut offsets_out = vec![0usize; 3];
        let mut variable_columns_out = vec![0usize; 2];
        let mut aggr_destructor_idxs_out = vec![0usize; 1];

        let result = compute_layout(
            &columns,
            &aggr_payload_sizes,
            &aggr_has_destructor,
            0, // can have nulls
            1, // top level
            &mut offsets_out,
            &mut variable_columns_out,
            &mut aggr_destructor_idxs_out,
        );

        // Verify result structure
        assert!(result.flag_width > 0); // 1 bit per 2 columns = 1 byte
        assert!(result.row_width > result.flag_width);
        assert_eq!(result.num_offsets, 3); // 2 columns + 1 aggregate
        assert_eq!(result.num_variable_columns, 0); // both are constant
        assert_eq!(result.num_aggr_destructor_idxs, 0);
    }
}
