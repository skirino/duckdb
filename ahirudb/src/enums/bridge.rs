pub fn catalog_type_to_string(t: u8) -> &'static str {
    match t {
        1 => "Table",
        2 => "Schema",
        3 => "View",
        4 => "Index",
        5 => "Prepared Statement",
        6 => "Sequence",
        7 => "Collation",
        8 => "Type",
        9 => "Database",
        10 => "Coordinate System",
        11 => "Trigger",
        25 => "Table Function",
        26 => "Scalar Function",
        27 => "Aggregate Function",
        28 => "Pragma Function",
        29 => "Copy Function",
        30 => "Macro Function",
        31 => "Table Macro Function",
        32 => "Window Function",
        71 => "Secret",
        72 => "Secret Type",
        73 => "Secret Function",
        _ => "INVALID",
    }
}

pub fn catalog_type_from_string(input: &[u8]) -> i8 {
    match input {
        b"INVALID" => 0,
        b"Collation" => 7,
        b"Coordinate System" => 10,
        b"Type" => 8,
        b"Table" => 1,
        b"Schema" => 2,
        b"Database" => 9,
        b"Table Function" => 25,
        b"Scalar Function" => 26,
        b"Aggregate Function" => 27,
        b"Window Function" => 32,
        b"Copy Function" => 29,
        b"Pragma Function" => 28,
        b"Macro Function" => 30,
        b"Table Macro Function" => 31,
        b"View" => 3,
        b"Index" => 4,
        b"Prepared Statement" => 5,
        b"Sequence" => 6,
        b"Trigger" => 11,
        b"Secret" => 71,
        b"Secret Type" => 72,
        b"Secret Function" => 73,
        _ => -1,
    }
}

pub fn compression_type_to_string(t: u8) -> &'static str {
    match t {
        0 => "Auto",
        1 => "Uncompressed",
        2 => "Constant",
        3 => "RLE",
        4 => "Dictionary",
        5 => "PFOR",
        6 => "BitPacking",
        7 => "FSST",
        8 => "Chimp",
        9 => "Patas",
        10 => "ALP",
        11 => "ALPRD",
        12 => "ZSTD",
        13 => "Roaring",
        14 => "Empty Validity",
        15 => "DICT_FSST",
        _ => "",
    }
}

pub fn try_date_part_specifier(input: &[u8]) -> i16 {
    let mut lower = Vec::with_capacity(input.len());
    for &b in input {
        lower.push(b.to_ascii_lowercase());
    }
    match lower.as_slice() {
        b"year" => 0,
        b"yr" => 0,
        b"y" => 0,
        b"years" => 0,
        b"yrs" => 0,
        b"month" => 1,
        b"mon" => 1,
        b"months" => 1,
        b"mons" => 1,
        b"day" => 2,
        b"days" => 2,
        b"d" => 2,
        b"dayofmonth" => 2,
        b"decade" => 3,
        b"dec" => 3,
        b"decades" => 3,
        b"decs" => 3,
        b"century" => 4,
        b"cent" => 4,
        b"centuries" => 4,
        b"c" => 4,
        b"millennium" => 5,
        b"mil" => 5,
        b"millenniums" => 5,
        b"millennia" => 5,
        b"mils" => 5,
        b"millenium" => 5,
        b"microseconds" => 6,
        b"microsecond" => 6,
        b"us" => 6,
        b"usec" => 6,
        b"usecs" => 6,
        b"usecond" => 6,
        b"useconds" => 6,
        b"milliseconds" => 7,
        b"millisecond" => 7,
        b"ms" => 7,
        b"msec" => 7,
        b"msecs" => 7,
        b"msecond" => 7,
        b"mseconds" => 7,
        b"second" => 8,
        b"sec" => 8,
        b"seconds" => 8,
        b"secs" => 8,
        b"s" => 8,
        b"minute" => 9,
        b"min" => 9,
        b"minutes" => 9,
        b"mins" => 9,
        b"m" => 9,
        b"hour" => 10,
        b"hr" => 10,
        b"hours" => 10,
        b"hrs" => 10,
        b"h" => 10,
        b"epoch" => 22,
        b"dow" => 11,
        b"dayofweek" => 11,
        b"weekday" => 11,
        b"isodow" => 12,
        b"week" => 13,
        b"weeks" => 13,
        b"w" => 13,
        b"weekofyear" => 13,
        b"doy" => 16,
        b"dayofyear" => 16,
        b"quarter" => 15,
        b"quarters" => 15,
        b"yearweek" => 17,
        b"isoyear" => 14,
        b"era" => 18,
        b"timezone" => 19,
        b"timezone_hour" => 20,
        b"timezone_minute" => 21,
        b"julian" => 23,
        b"jd" => 23,
        _ => -1,
    }
}

pub fn is_bigint_datepart(t: u8) -> bool {
    t < 22
}

pub fn expression_type_to_string(t: u8) -> &'static str {
    match t {
        12 => "CAST",
        13 => "NOT",
        14 => "IS_NULL",
        15 => "IS_NOT_NULL",
        16 => "UNPACK",
        25 => "EQUAL",
        26 => "NOTEQUAL",
        27 => "LESSTHAN",
        28 => "GREATERTHAN",
        29 => "LESSTHANOREQUALTO",
        30 => "GREATERTHANOREQUALTO",
        35 => "IN",
        36 => "COMPARE_NOT_IN",
        37 => "DISTINCT_FROM",
        38 => "COMPARE_BETWEEN",
        39 => "COMPARE_NOT_BETWEEN",
        40 => "NOT_DISTINCT_FROM",
        50 => "AND",
        51 => "OR",
        75 => "CONSTANT",
        76 => "PARAMETER",
        77 => "TUPLE",
        78 => "TUPLE_ADDRESS",
        79 => "NULL",
        80 => "VECTOR",
        81 => "SCALAR",
        82 => "VALUE_DEFAULT",
        100 => "AGGREGATE",
        101 => "BOUND_AGGREGATE",
        102 => "GROUPING",
        110 => "WINDOW_AGGREGATE",
        111 => "WINDOW_FUNCTION",
        120 => "RANK",
        121 => "RANK_DENSE",
        122 => "NTILE",
        123 => "PERCENT_RANK",
        124 => "CUME_DIST",
        125 => "ROW_NUMBER",
        130 => "FIRST_VALUE",
        131 => "LAST_VALUE",
        132 => "LEAD",
        133 => "LAG",
        134 => "NTH_VALUE",
        135 => "FILL",
        140 => "FUNCTION",
        141 => "BOUND_FUNCTION",
        150 => "CASE",
        151 => "NULLIF",
        152 => "COALESCE",
        153 => "ARRAY_EXTRACT",
        154 => "ARRAY_SLICE",
        155 => "STRUCT_EXTRACT",
        156 => "ARRAY_CONSTRUCTOR",
        157 => "ARROW",
        158 => "TRY",
        175 => "SUBQUERY",
        200 => "STAR",
        201 => "TABLE_STAR",
        202 => "PLACEHOLDER",
        203 => "COLUMN_REF",
        204 => "FUNCTION_REF",
        205 => "TABLE_REF",
        206 => "LAMBDA_REF",
        207 => "TYPE",
        225 => "CAST",
        227 => "BOUND_REF",
        228 => "BOUND_COLUMN_REF",
        229 => "BOUND_UNNEST",
        230 => "COLLATE",
        231 => "LAMBDA",
        232 => "POSITIONAL_REFERENCE",
        233 => "BOUND_LAMBDA_REF",
        234 => "BOUND_EXPANDED",
        _ => "INVALID",
    }
}

pub fn expression_class_to_string(t: u8) -> &'static str {
    match t {
        0 => "INVALID",
        1 => "AGGREGATE",
        2 => "CASE",
        3 => "CAST",
        4 => "COLUMN_REF",
        5 => "COMPARISON",
        6 => "CONJUNCTION",
        7 => "CONSTANT",
        8 => "DEFAULT",
        9 => "FUNCTION",
        10 => "OPERATOR",
        11 => "STAR",
        13 => "SUBQUERY",
        14 => "WINDOW",
        15 => "PARAMETER",
        16 => "COLLATE",
        17 => "LAMBDA",
        18 => "POSITIONAL_REFERENCE",
        19 => "BETWEEN",
        20 => "LAMBDA_REF",
        21 => "TYPE",
        25 => "BOUND_AGGREGATE",
        26 => "BOUND_CASE",
        27 => "LEGACY_BOUND_CAST",
        28 => "BOUND_COLUMN_REF",
        29 => "LEGACY_BOUND_COMPARISON",
        30 => "BOUND_CONJUNCTION",
        31 => "BOUND_CONSTANT",
        32 => "BOUND_DEFAULT",
        33 => "BOUND_FUNCTION",
        34 => "BOUND_OPERATOR",
        35 => "BOUND_PARAMETER",
        36 => "BOUND_REF",
        37 => "BOUND_SUBQUERY",
        38 => "BOUND_WINDOW",
        39 => "LEGACY_BOUND_BETWEEN",
        40 => "BOUND_UNNEST",
        41 => "BOUND_LAMBDA",
        51 => "BOUND_EXPANDED",
        _ => "ExpressionClass::!!UNIMPLEMENTED_CASE!!",
    }
}

pub fn expression_type_to_operator(t: u8) -> &'static str {
    match t {
        25 => "=",
        26 => "!=",
        27 => "<",
        28 => ">",
        29 => "<=",
        30 => ">=",
        37 => "IS DISTINCT FROM",
        40 => "IS NOT DISTINCT FROM",
        50 => "AND",
        51 => "OR",
        _ => "",
    }
}

pub fn operator_to_expression_type(op: &[u8]) -> i8 {
    if op == b"=" || op == b"==" {
        25
    } else if op == b"!=" || op == b"<>" {
        26
    } else if op == b"<" {
        27
    } else if op == b">" {
        28
    } else if op == b"<=" {
        29
    } else if op == b">=" {
        30
    } else {
        0
    }
}

pub fn negate_comparison_expression(t: u8) -> i8 {
    match t {
        25 => 26,
        26 => 25,
        27 => 30,
        28 => 29,
        29 => 28,
        30 => 27,
        37 => 40,
        40 => 37,
        _ => -1,
    }
}

pub fn flip_comparison_expression(t: u8) -> i8 {
    match t {
        40 => 40,
        37 => 37,
        26 => 26,
        25 => 25,
        27 => 28,
        28 => 27,
        29 => 30,
        30 => 29,
        _ => -1,
    }
}

pub fn http_status_code_from_int(t: i32) -> i32 {
    if !(100..=599).contains(&t) { 0 } else { t }
}

pub fn http_status_message(t: u16) -> &'static str {
    match t {
        100 => "Continue",
        101 => "Switching Protocol",
        102 => "Processing",
        103 => "Early Hints",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        205 => "Reset Content",
        206 => "Partial Content",
        207 => "Multi-Status",
        208 => "Already Reported",
        226 => "IM Used",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        305 => "Use Proxy",
        306 => "unused",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        417 => "Expectation Failed",
        418 => "I'm a teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Content",
        423 => "Locked",
        424 => "Failed Dependency",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        506 => "Variant Also Negotiates",
        507 => "Insufficient Storage",
        508 => "Loop Detected",
        510 => "Not Extended",
        511 => "Network Authentication Required",
        _ => "Unknown",
    }
}

pub fn is_left_outer_join(t: u8) -> bool {
    t == 1 || t == 4
}
pub fn is_right_outer_join(t: u8) -> bool {
    t == 4 || t == 2
}
pub fn propagates_build_side(t: u8) -> bool {
    t == 4 || t == 2 || t == 9 || t == 10
}
pub fn has_inverse_join_type(t: u8) -> bool {
    t != 7 && t != 8
}
pub fn inverse_join_type(t: u8) -> i8 {
    match t {
        1 => 2,
        2 => 1,
        3 => 3,
        4 => 4,
        5 => 9,
        6 => 10,
        9 => 5,
        10 => 6,
        _ => -1,
    }
}

pub fn join_type_to_string(t: u8) -> &'static str {
    match t {
        0 => "INVALID",
        1 => "LEFT",
        2 => "RIGHT",
        3 => "INNER",
        4 => "FULL",
        5 => "SEMI",
        6 => "ANTI",
        7 => "MARK",
        8 => "SINGLE",
        9 => "RIGHT_SEMI",
        10 => "RIGHT_ANTI",
        _ => "",
    }
}

pub fn logical_operator_to_string(t: u8) -> &'static str {
    match t {
        1 => "PROJECTION",
        2 => "FILTER",
        3 => "AGGREGATE",
        4 => "WINDOW",
        5 => "UNNEST",
        6 => "LIMIT",
        7 => "ORDER_BY",
        8 => "TOP_N",
        10 => "COPY_TO_FILE",
        11 => "DISTINCT",
        12 => "SAMPLE",
        14 => "PIVOT",
        15 => "COPY_DATABASE",
        16 => "SECURE_VIEW",
        25 => "GET",
        26 => "CHUNK_GET",
        27 => "DELIM_GET",
        28 => "EXPRESSION_GET",
        29 => "DUMMY_SCAN",
        30 => "EMPTY_RESULT",
        31 => "CTE_SCAN",
        50 => "JOIN",
        51 => "DELIM_JOIN",
        52 => "COMPARISON_JOIN",
        53 => "ANY_JOIN",
        54 => "CROSS_PRODUCT",
        55 => "POSITIONAL_JOIN",
        56 => "ASOF_JOIN",
        57 => "DEPENDENT_JOIN",
        75 => "UNION",
        76 => "EXCEPT",
        77 => "INTERSECT",
        78 => "REC_CTE",
        79 => "CTE",
        100 => "INSERT",
        101 => "DELETE",
        102 => "UPDATE",
        103 => "MERGE_INTO",
        104 => "TRIGGER",
        125 => "ALTER",
        126 => "CREATE_TABLE",
        127 => "CREATE_INDEX",
        128 => "CREATE_SEQUENCE",
        129 => "CREATE_VIEW",
        130 => "CREATE_SCHEMA",
        131 => "CREATE_MACRO",
        132 => "DROP",
        133 => "PRAGMA",
        134 => "TRANSACTION",
        135 => "CREATE_TYPE",
        136 => "ATTACH",
        137 => "DETACH",
        138 => "CREATE_TRIGGER",
        150 => "EXPLAIN",
        175 => "PREPARE",
        176 => "EXECUTE",
        177 => "EXPORT",
        178 => "VACUUM",
        179 => "SET",
        180 => "LOAD",
        181 => "RESET",
        182 => "UPDATE_EXTENSIONS",
        183 => "CONNECT",
        184 => "DISCONNECT",
        185 => "EXTERNAL_RESOURCE",
        190 => "CREATE_SECRET",
        255 => "CUSTOM_OP",
        _ => "INVALID",
    }
}

static OPTIMIZER_TABLE: &[(&str, u32)] = &[
    ("expression_rewriter", 1),
    ("filter_pullup", 2),
    ("filter_pushdown", 3),
    ("empty_result_pullup", 4),
    ("cte_filter_pusher", 5),
    ("regex_range", 6),
    ("in_clause", 7),
    ("join_order", 8),
    ("deliminator", 9),
    ("unnest_rewriter", 10),
    ("unused_columns", 11),
    ("statistics_propagation", 12),
    ("common_subexpressions", 13),
    ("common_aggregate", 14),
    ("column_lifetime", 15),
    ("limit_pushdown", 17),
    ("row_group_pruner", 29),
    ("top_n", 18),
    ("top_n_window_elimination", 30),
    ("build_side_probe_side", 16),
    ("compressed_materialization", 19),
    ("duplicate_groups", 20),
    ("reorder_filter", 21),
    ("sampling_pushdown", 22),
    ("join_filter_pushdown", 23),
    ("extension", 24),
    ("materialized_cte", 25),
    ("aggregate_function_rewriter", 26),
    ("late_materialization", 27),
    ("cte_inlining", 28),
    ("common_subplan", 31),
    ("join_elimination", 32),
    ("window_self_join", 33),
    ("projection_pullup", 34),
    ("outer_join_simplification", 35),
    ("window_rewriter", 36),
    ("partitioned_execution", 37),
    ("partial_aggregate_pushdown", 38),
    ("remote_pushdown", 39),
    ("grouping_sets", 40),
    ("type_pushdown", 41),
    ("scalar_fn_pushdown", 42),
    ("distinct_aggregate_rewrite", 43),
    ("aggregate_reuse", 44),
    ("projection_placement", 45),
];

pub fn optimizer_count() -> usize {
    OPTIMIZER_TABLE.len()
}
pub fn optimizer_name(idx: usize) -> &'static str {
    if idx < OPTIMIZER_TABLE.len() {
        OPTIMIZER_TABLE[idx].0
    } else {
        ""
    }
}
pub fn optimizer_name_for_discriminant(d: u32) -> &'static str {
    for &(name, disc) in OPTIMIZER_TABLE {
        if disc == d {
            return name;
        }
    }
    ""
}
pub fn optimizer_discriminant_of_name(input: &[u8]) -> i64 {
    for &(name, disc) in OPTIMIZER_TABLE {
        if input == name.as_bytes() {
            return disc as i64;
        }
    }
    -1
}

pub fn physical_operator_to_string(t: u8) -> &'static str {
    match t {
        1 => "ORDER_BY",
        2 => "LIMIT",
        3 => "STREAMING_LIMIT",
        4 => "LIMIT_PERCENT",
        5 => "TOP_N",
        6 => "WINDOW",
        7 => "UNNEST",
        8 => "UNGROUPED_AGGREGATE",
        9 => "HASH_GROUP_BY",
        10 => "PERFECT_HASH_GROUP_BY",
        11 => "PARTITIONED_AGGREGATE",
        12 => "LIMITED_DISTINCT",
        13 => "FILTER",
        14 => "PROJECTION",
        15 => "COPY_TO_FILE",
        16 => "BATCH_COPY_TO_FILE",
        17 => "RESERVOIR_SAMPLE",
        18 => "STREAMING_SAMPLE",
        19 => "STREAMING_WINDOW",
        20 => "PIVOT",
        21 => "COPY_DATABASE",
        22 => "TABLE_SCAN",
        23 => "DUMMY_SCAN",
        24 => "COLUMN_DATA_SCAN",
        25 => "CHUNK_SCAN",
        26 => "REC_CTE_SCAN",
        27 => "REC_REC_CTE_SCAN",
        28 => "CTE_SCAN",
        29 => "DELIM_SCAN",
        30 => "EXPRESSION_SCAN",
        31 => "POSITIONAL_SCAN",
        32 => "BLOCKWISE_NL_JOIN",
        33 => "NESTED_LOOP_JOIN",
        34 => "HASH_JOIN",
        35 => "CROSS_PRODUCT",
        36 => "PIECEWISE_MERGE_JOIN",
        37 => "IE_JOIN",
        38 => "LEFT_DELIM_JOIN",
        39 => "RIGHT_DELIM_JOIN",
        40 => "POSITIONAL_JOIN",
        41 => "ASOF_JOIN",
        42 => "UNION",
        43 => "REC_CTE",
        44 => "REC_KEY_CTE",
        45 => "CTE",
        46 => "INSERT",
        47 => "BATCH_INSERT",
        48 => "DELETE",
        49 => "UPDATE",
        50 => "MERGE_INTO",
        51 => "CREATE_TABLE",
        52 => "CREATE_TABLE_AS",
        53 => "BATCH_CREATE_TABLE_AS",
        54 => "CREATE_INDEX",
        55 => "ALTER",
        56 => "CREATE_SEQUENCE",
        57 => "CREATE_VIEW",
        58 => "CREATE_SCHEMA",
        59 => "CREATE_MACRO",
        60 => "DROP",
        61 => "PRAGMA",
        62 => "TRANSACTION",
        63 => "CREATE_TYPE",
        64 => "CREATE_TRIGGER",
        65 => "ATTACH",
        66 => "DETACH",
        67 => "EXPLAIN",
        68 => "EXPLAIN_ANALYZE",
        69 => "EMPTY_RESULT",
        70 => "EXECUTE",
        71 => "PREPARE",
        72 => "VACUUM",
        73 => "EXPORT",
        74 => "SET",
        75 => "SET_VARIABLE",
        76 => "LOAD",
        77 => "INOUT_FUNCTION",
        78 => "RESULT_COLLECTOR",
        79 => "RESET",
        80 => "EXTENSION",
        81 => "VERIFY_VECTOR",
        82 => "UPDATE_EXTENSIONS",
        83 => "CONNECT",
        84 => "DISCONNECT",
        85 => "EXTERNAL_RESOURCE",
        86 => "CREATE_SECRET",
        87 => "RECURSIVE_KEY_JOIN",
        88 => "SECURE_VIEW",
        89 => "MERGE_ACTION_SOURCE",
        _ => "INVALID",
    }
}

pub fn relation_type_to_string(t: u8) -> &'static str {
    match t {
        1 => "TABLE_RELATION",
        2 => "PROJECTION_RELATION",
        3 => "FILTER_RELATION",
        4 => "EXPLAIN_RELATION",
        5 => "CROSS_PRODUCT_RELATION",
        6 => "JOIN_RELATION",
        7 => "AGGREGATE_RELATION",
        8 => "SET_OPERATION_RELATION",
        9 => "DISTINCT_RELATION",
        10 => "LIMIT_RELATION",
        11 => "ORDER_RELATION",
        12 => "CREATE_VIEW_RELATION",
        13 => "CREATE_TABLE_RELATION",
        14 => "INSERT_RELATION",
        15 => "VALUE_LIST_RELATION",
        16 => "MATERIALIZED_RELATION",
        17 => "DELETE_RELATION",
        18 => "UPDATE_RELATION",
        19 => "WRITE_CSV_RELATION",
        20 => "WRITE_PARQUET_RELATION",
        21 => "READ_CSV_RELATION",
        22 => "SUBQUERY_RELATION",
        23 => "TABLE_FUNCTION_RELATION",
        24 => "VIEW_RELATION",
        25 => "QUERY_RELATION",
        26 => "DELIM_JOIN_RELATION",
        27 => "DELIM_GET_RELATION",
        255 => "EXTENSION_RELATION",
        _ => "INVALID_RELATION",
    }
}

pub fn statement_type_to_string(t: u8) -> &'static str {
    match t {
        1 => "SELECT",
        2 => "INSERT",
        3 => "UPDATE",
        4 => "CREATE",
        5 => "DELETE",
        6 => "PREPARE",
        7 => "EXECUTE",
        8 => "ALTER",
        9 => "TRANSACTION",
        10 => "COPY",
        11 => "ANALYZE",
        12 => "VARIABLE_SET",
        13 => "CREATE_FUNC",
        14 => "EXPLAIN",
        15 => "DROP",
        16 => "EXPORT",
        17 => "PRAGMA",
        18 => "VACUUM",
        19 => "CALL",
        20 => "SET",
        21 => "LOAD",
        22 => "RELATION",
        23 => "EXTENSION",
        24 => "LOGICAL_PLAN",
        25 => "ATTACH",
        26 => "DETACH",
        27 => "MULTI",
        28 => "COPY_DATABASE",
        29 => "UPDATE_EXTENSIONS",
        30 => "MERGE_INTO",
        31 => "CONNECT",
        32 => "DISCONNECT",
        33 => "EXTERNAL_RESOURCE",
        _ => "INVALID",
    }
}

pub fn statement_return_type_to_string(t: u8) -> &'static str {
    match t {
        0 => "QUERY_RESULT",
        1 => "CHANGED_ROWS",
        2 => "NOTHING",
        _ => "INVALID",
    }
}

// Every table below is the C++ original's `switch` transcribed, so the tests
// pin the Rust port to the strings the C++ produced rather than to each other.
// The discriminants are the ones in `src/include/duckdb/common/enums/*.hpp`; a
// change there has to change the expectation here too.
#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Checks a label lookup against every `(discriminant, name)` pair it is
    /// given.
    fn check_labels<T: Copy + std::fmt::Debug>(
        lookup: impl Fn(T) -> &'static str,
        cases: &[(T, &str)],
    ) {
        for &(discriminant, expected) in cases {
            assert_eq!(
                lookup(discriminant),
                expected,
                "discriminant {discriminant:?}"
            );
        }
    }

    //===--------------------------------------------------------------------===//
    // CatalogType
    //===--------------------------------------------------------------------===//

    const CATALOG_TYPE_LABELS: &[(u8, &str)] = &[
        (1, "Table"),
        (2, "Schema"),
        (3, "View"),
        (4, "Index"),
        (5, "Prepared Statement"),
        (6, "Sequence"),
        (7, "Collation"),
        (8, "Type"),
        (9, "Database"),
        (10, "Coordinate System"),
        (11, "Trigger"),
        (25, "Table Function"),
        (26, "Scalar Function"),
        (27, "Aggregate Function"),
        (28, "Pragma Function"),
        (29, "Copy Function"),
        (30, "Macro Function"),
        (31, "Table Macro Function"),
        (32, "Window Function"),
        (71, "Secret"),
        (72, "Secret Type"),
        (73, "Secret Function"),
    ];

    #[test]
    fn every_catalog_type_label_round_trips() {
        for &(discriminant, label) in CATALOG_TYPE_LABELS {
            assert_eq!(catalog_type_to_string(discriminant), label);
            assert_eq!(
                catalog_type_from_string(label.as_bytes()),
                discriminant as i8
            );
        }
    }

    #[test]
    fn catalog_type_from_string_accepts_invalid_and_rejects_unknown() {
        // The original had an explicit `type == "INVALID"` branch; the other
        // entries that stringify to "INVALID" are not names.
        assert_eq!(catalog_type_from_string(b"INVALID"), 0);
        assert_eq!(catalog_type_from_string(b"nonsense"), -1);
        assert_eq!(catalog_type_from_string(b"table"), -1);
    }

    #[test]
    fn catalog_types_without_a_label_are_invalid() {
        for discriminant in [0u8, 12, 24, 33, 50, 51, 52, 53, 70, 100, 255] {
            assert_eq!(catalog_type_to_string(discriminant), "INVALID");
        }
    }

    //===--------------------------------------------------------------------===//
    // CompressionType
    //===--------------------------------------------------------------------===//

    #[test]
    fn every_compression_type_label_matches_the_cpp() {
        check_labels(
            compression_type_to_string,
            &[
                (0, "Auto"),
                (1, "Uncompressed"),
                (2, "Constant"),
                (3, "RLE"),
                (4, "Dictionary"),
                (5, "PFOR"),
                (6, "BitPacking"),
                (7, "FSST"),
                (8, "Chimp"),
                (9, "Patas"),
                (10, "ALP"),
                (11, "ALPRD"),
                (12, "ZSTD"),
                (13, "Roaring"),
                (14, "Empty Validity"),
                (15, "DICT_FSST"),
            ],
        );
    }

    #[test]
    fn an_unknown_compression_type_has_an_empty_name() {
        // The empty string is what makes the C++ shell throw the
        // `Unrecognized compression type!` the original `default:` arm threw.
        assert_eq!(compression_type_to_string(16), "");
        assert_eq!(compression_type_to_string(255), "");
    }

    //===--------------------------------------------------------------------===//
    // DatePartSpecifier
    //===--------------------------------------------------------------------===//

    const DATE_PART_ALIASES: &[(&str, i16)] = &[
        ("year", 0),
        ("yr", 0),
        ("y", 0),
        ("years", 0),
        ("yrs", 0),
        ("month", 1),
        ("mon", 1),
        ("months", 1),
        ("mons", 1),
        ("day", 2),
        ("days", 2),
        ("d", 2),
        ("dayofmonth", 2),
        ("decade", 3),
        ("dec", 3),
        ("decades", 3),
        ("decs", 3),
        ("century", 4),
        ("cent", 4),
        ("centuries", 4),
        ("c", 4),
        ("millennium", 5),
        ("mil", 5),
        ("millenniums", 5),
        ("millennia", 5),
        ("mils", 5),
        ("millenium", 5),
        ("microseconds", 6),
        ("microsecond", 6),
        ("us", 6),
        ("usec", 6),
        ("usecs", 6),
        ("usecond", 6),
        ("useconds", 6),
        ("milliseconds", 7),
        ("millisecond", 7),
        ("ms", 7),
        ("msec", 7),
        ("msecs", 7),
        ("msecond", 7),
        ("mseconds", 7),
        ("second", 8),
        ("sec", 8),
        ("seconds", 8),
        ("secs", 8),
        ("s", 8),
        ("minute", 9),
        ("min", 9),
        ("minutes", 9),
        ("mins", 9),
        ("m", 9),
        ("hour", 10),
        ("hr", 10),
        ("hours", 10),
        ("hrs", 10),
        ("h", 10),
        ("epoch", 22),
        ("dow", 11),
        ("dayofweek", 11),
        ("weekday", 11),
        ("isodow", 12),
        ("week", 13),
        ("weeks", 13),
        ("w", 13),
        ("weekofyear", 13),
        ("doy", 16),
        ("dayofyear", 16),
        ("quarter", 15),
        ("quarters", 15),
        ("yearweek", 17),
        ("isoyear", 14),
        ("era", 18),
        ("timezone", 19),
        ("timezone_hour", 20),
        ("timezone_minute", 21),
        ("julian", 23),
        ("jd", 23),
    ];

    #[test]
    fn every_date_part_alias_maps_to_its_specifier() {
        for &(alias, expected) in DATE_PART_ALIASES {
            assert_eq!(
                try_date_part_specifier(alias.as_bytes()),
                expected,
                "alias {alias}"
            );
        }
    }

    #[test]
    fn date_part_aliases_are_case_insensitive() {
        assert_eq!(try_date_part_specifier(b"YEAR"), 0);
        assert_eq!(try_date_part_specifier(b"Year"), 0);
        assert_eq!(try_date_part_specifier(b"MONTH"), 1);
        assert_eq!(try_date_part_specifier(b"Quarter"), 15);
    }

    #[test]
    fn an_unknown_date_part_has_no_specifier() {
        assert_eq!(try_date_part_specifier(b"nonsense"), -1);
        assert_eq!(try_date_part_specifier(b""), -1);
    }

    #[test]
    fn is_bigint_datepart_covers_the_bigint_range() {
        // DatePartSpecifier::BEGIN_DOUBLE is EPOCH = 22.
        for discriminant in 0u8..22 {
            assert!(
                is_bigint_datepart(discriminant),
                "discriminant {discriminant}"
            );
        }
        for discriminant in [22u8, 23, 24, 25, 255] {
            assert!(
                !is_bigint_datepart(discriminant),
                "discriminant {discriminant}"
            );
        }
    }

    //===--------------------------------------------------------------------===//
    // ExpressionType / ExpressionClass
    //===--------------------------------------------------------------------===//

    #[test]
    fn every_expression_type_label_matches_the_cpp() {
        check_labels(
            expression_type_to_string,
            &[
                (12, "CAST"),
                (13, "NOT"),
                (14, "IS_NULL"),
                (15, "IS_NOT_NULL"),
                (16, "UNPACK"),
                (25, "EQUAL"),
                (26, "NOTEQUAL"),
                (27, "LESSTHAN"),
                (28, "GREATERTHAN"),
                (29, "LESSTHANOREQUALTO"),
                (30, "GREATERTHANOREQUALTO"),
                (35, "IN"),
                (36, "COMPARE_NOT_IN"),
                (37, "DISTINCT_FROM"),
                (38, "COMPARE_BETWEEN"),
                (39, "COMPARE_NOT_BETWEEN"),
                (40, "NOT_DISTINCT_FROM"),
                (50, "AND"),
                (51, "OR"),
                (75, "CONSTANT"),
                (76, "PARAMETER"),
                (77, "TUPLE"),
                (78, "TUPLE_ADDRESS"),
                (79, "NULL"),
                (80, "VECTOR"),
                (81, "SCALAR"),
                (82, "VALUE_DEFAULT"),
                (100, "AGGREGATE"),
                (101, "BOUND_AGGREGATE"),
                (102, "GROUPING"),
                (110, "WINDOW_AGGREGATE"),
                (111, "WINDOW_FUNCTION"),
                (120, "RANK"),
                (121, "RANK_DENSE"),
                (122, "NTILE"),
                (123, "PERCENT_RANK"),
                (124, "CUME_DIST"),
                (125, "ROW_NUMBER"),
                (130, "FIRST_VALUE"),
                (131, "LAST_VALUE"),
                (132, "LEAD"),
                (133, "LAG"),
                (134, "NTH_VALUE"),
                (135, "FILL"),
                (140, "FUNCTION"),
                (141, "BOUND_FUNCTION"),
                (150, "CASE"),
                (151, "NULLIF"),
                (152, "COALESCE"),
                (153, "ARRAY_EXTRACT"),
                (154, "ARRAY_SLICE"),
                (155, "STRUCT_EXTRACT"),
                (156, "ARRAY_CONSTRUCTOR"),
                (157, "ARROW"),
                (158, "TRY"),
                (175, "SUBQUERY"),
                (200, "STAR"),
                (201, "TABLE_STAR"),
                (202, "PLACEHOLDER"),
                (203, "COLUMN_REF"),
                (204, "FUNCTION_REF"),
                (205, "TABLE_REF"),
                (206, "LAMBDA_REF"),
                (207, "TYPE"),
                (225, "CAST"),
                (227, "BOUND_REF"),
                (228, "BOUND_COLUMN_REF"),
                (229, "BOUND_UNNEST"),
                (230, "COLLATE"),
                (231, "LAMBDA"),
                (232, "POSITIONAL_REFERENCE"),
                (233, "BOUND_LAMBDA_REF"),
                (234, "BOUND_EXPANDED"),
            ],
        );
    }

    #[test]
    fn comparison_expression_types_keep_their_own_labels() {
        // `expression_type_to_string` answers the enum's label; only
        // `expression_type_to_operator` answers the operator spelling, and the
        // two must not be swapped.
        check_labels(
            expression_type_to_string,
            &[
                (25, "EQUAL"),
                (26, "NOTEQUAL"),
                (27, "LESSTHAN"),
                (28, "GREATERTHAN"),
                (29, "LESSTHANOREQUALTO"),
                (30, "GREATERTHANOREQUALTO"),
                (37, "DISTINCT_FROM"),
                (40, "NOT_DISTINCT_FROM"),
            ],
        );
        check_labels(
            expression_type_to_operator,
            &[
                (25, "="),
                (26, "!="),
                (27, "<"),
                (28, ">"),
                (29, "<="),
                (30, ">="),
                (37, "IS DISTINCT FROM"),
                (40, "IS NOT DISTINCT FROM"),
                (50, "AND"),
                (51, "OR"),
            ],
        );
        assert_eq!(expression_type_to_operator(12), "");
    }

    #[test]
    fn every_expression_class_label_matches_the_cpp() {
        check_labels(
            expression_class_to_string,
            &[
                (0, "INVALID"),
                (1, "AGGREGATE"),
                (2, "CASE"),
                (3, "CAST"),
                (4, "COLUMN_REF"),
                (5, "COMPARISON"),
                (6, "CONJUNCTION"),
                (7, "CONSTANT"),
                (8, "DEFAULT"),
                (9, "FUNCTION"),
                (10, "OPERATOR"),
                (11, "STAR"),
                (13, "SUBQUERY"),
                (14, "WINDOW"),
                (15, "PARAMETER"),
                (16, "COLLATE"),
                (17, "LAMBDA"),
                (18, "POSITIONAL_REFERENCE"),
                (19, "BETWEEN"),
                (20, "LAMBDA_REF"),
                (21, "TYPE"),
                (25, "BOUND_AGGREGATE"),
                (26, "BOUND_CASE"),
                (27, "LEGACY_BOUND_CAST"),
                (28, "BOUND_COLUMN_REF"),
                (29, "LEGACY_BOUND_COMPARISON"),
                (30, "BOUND_CONJUNCTION"),
                (31, "BOUND_CONSTANT"),
                (32, "BOUND_DEFAULT"),
                (33, "BOUND_FUNCTION"),
                (34, "BOUND_OPERATOR"),
                (35, "BOUND_PARAMETER"),
                (36, "BOUND_REF"),
                (37, "BOUND_SUBQUERY"),
                (38, "BOUND_WINDOW"),
                (39, "LEGACY_BOUND_BETWEEN"),
                (40, "BOUND_UNNEST"),
                (41, "BOUND_LAMBDA"),
                (51, "BOUND_EXPANDED"),
            ],
        );
        // 42 (BOUND_LAMBDA_REF) is deliberately absent in the C++ switch too,
        // so it answers the same unimplemented-case string there.
        assert_eq!(
            expression_class_to_string(42),
            "ExpressionClass::!!UNIMPLEMENTED_CASE!!"
        );
        assert_eq!(
            expression_class_to_string(255),
            "ExpressionClass::!!UNIMPLEMENTED_CASE!!"
        );
    }

    #[test]
    fn operator_to_expression_type_is_the_inverse_of_expression_type_to_operator() {
        for &(discriminant, operator) in &[
            (25u8, "="),
            (26, "!="),
            (27, "<"),
            (28, ">"),
            (29, "<="),
            (30, ">="),
        ] {
            assert_eq!(expression_type_to_operator(discriminant), operator);
            assert_eq!(
                operator_to_expression_type(operator.as_bytes()),
                discriminant as i8
            );
        }
        assert_eq!(operator_to_expression_type(b"=="), 25);
        assert_eq!(operator_to_expression_type(b"<>"), 26);
        assert_eq!(operator_to_expression_type(b"nonsense"), 0);
    }

    #[test]
    fn negate_and_flip_comparison_expressions() {
        for &(input, negated, flipped) in &[
            (25u8, 26i8, 25i8),
            (26, 25, 26),
            (27, 30, 28),
            (28, 29, 27),
            (29, 28, 30),
            (30, 27, 29),
            (37, 40, 37),
            (40, 37, 40),
        ] {
            assert_eq!(
                negate_comparison_expression(input),
                negated,
                "negate {input}"
            );
            assert_eq!(flip_comparison_expression(input), flipped, "flip {input}");
        }
    }

    #[test]
    fn negate_and_flip_reject_non_comparisons() {
        assert_eq!(negate_comparison_expression(0), -1);
        assert_eq!(flip_comparison_expression(0), -1);
        assert_eq!(negate_comparison_expression(12), -1);
        assert_eq!(flip_comparison_expression(12), -1);
    }

    //===--------------------------------------------------------------------===//
    // HTTP status codes
    //===--------------------------------------------------------------------===//

    #[test]
    fn http_status_code_from_int_keeps_only_the_valid_range() {
        assert_eq!(http_status_code_from_int(-1), 0);
        assert_eq!(http_status_code_from_int(0), 0);
        assert_eq!(http_status_code_from_int(99), 0);
        assert_eq!(http_status_code_from_int(100), 100);
        assert_eq!(http_status_code_from_int(499), 499);
        assert_eq!(http_status_code_from_int(599), 599);
        assert_eq!(http_status_code_from_int(600), 0);
        assert_eq!(http_status_code_from_int(1000), 0);
    }

    #[test]
    fn every_http_status_message_matches_the_cpp() {
        check_labels(
            http_status_message,
            &[
                (100, "Continue"),
                (101, "Switching Protocol"),
                (102, "Processing"),
                (103, "Early Hints"),
                (200, "OK"),
                (201, "Created"),
                (202, "Accepted"),
                (203, "Non-Authoritative Information"),
                (204, "No Content"),
                (205, "Reset Content"),
                (206, "Partial Content"),
                (207, "Multi-Status"),
                (208, "Already Reported"),
                (226, "IM Used"),
                (300, "Multiple Choices"),
                (301, "Moved Permanently"),
                (302, "Found"),
                (303, "See Other"),
                (304, "Not Modified"),
                (305, "Use Proxy"),
                (306, "unused"),
                (307, "Temporary Redirect"),
                (308, "Permanent Redirect"),
                (400, "Bad Request"),
                (401, "Unauthorized"),
                (402, "Payment Required"),
                (403, "Forbidden"),
                (404, "Not Found"),
                (405, "Method Not Allowed"),
                (406, "Not Acceptable"),
                (407, "Proxy Authentication Required"),
                (408, "Request Timeout"),
                (409, "Conflict"),
                (410, "Gone"),
                (411, "Length Required"),
                (412, "Precondition Failed"),
                (413, "Payload Too Large"),
                (414, "URI Too Long"),
                (415, "Unsupported Media Type"),
                (416, "Range Not Satisfiable"),
                (417, "Expectation Failed"),
                (418, "I'm a teapot"),
                (421, "Misdirected Request"),
                (422, "Unprocessable Content"),
                (423, "Locked"),
                (424, "Failed Dependency"),
                (425, "Too Early"),
                (426, "Upgrade Required"),
                (428, "Precondition Required"),
                (429, "Too Many Requests"),
                (431, "Request Header Fields Too Large"),
                (451, "Unavailable For Legal Reasons"),
                (500, "Internal Server Error"),
                (501, "Not Implemented"),
                (502, "Bad Gateway"),
                (503, "Service Unavailable"),
                (504, "Gateway Timeout"),
                (505, "HTTP Version Not Supported"),
                (506, "Variant Also Negotiates"),
                (507, "Insufficient Storage"),
                (508, "Loop Detected"),
                (510, "Not Extended"),
                (511, "Network Authentication Required"),
            ],
        );
    }

    #[test]
    fn an_unknown_http_status_has_the_unknown_message() {
        assert_eq!(http_status_message(0), "Unknown");
        assert_eq!(http_status_message(200 - 1), "Unknown");
        assert_eq!(http_status_message(999), "Unknown");
    }

    //===--------------------------------------------------------------------===//
    // JoinType
    //===--------------------------------------------------------------------===//

    #[test]
    fn join_type_predicates_match_the_cpp() {
        // LEFT = 1, RIGHT = 2, INNER = 3, OUTER = 4, SEMI = 5, ANTI = 6,
        // MARK = 7, SINGLE = 8, RIGHT_SEMI = 9, RIGHT_ANTI = 10.
        for &join_type in &[1u8, 4] {
            assert!(is_left_outer_join(join_type), "left outer {join_type}");
        }
        for &join_type in &[0u8, 2, 3, 5, 6, 7, 8, 9, 10] {
            assert!(!is_left_outer_join(join_type), "left outer {join_type}");
        }

        for &join_type in &[4u8, 2] {
            assert!(is_right_outer_join(join_type), "right outer {join_type}");
        }
        for &join_type in &[0u8, 1, 3, 5, 6, 7, 8, 9, 10] {
            assert!(!is_right_outer_join(join_type), "right outer {join_type}");
        }

        for &join_type in &[4u8, 2, 10, 9] {
            assert!(propagates_build_side(join_type), "build side {join_type}");
        }
        for &join_type in &[0u8, 1, 3, 5, 6, 7, 8] {
            assert!(!propagates_build_side(join_type), "build side {join_type}");
        }

        assert!(!has_inverse_join_type(7));
        assert!(!has_inverse_join_type(8));
        for &join_type in &[0u8, 1, 2, 3, 4, 5, 6, 9, 10] {
            assert!(
                has_inverse_join_type(join_type),
                "inverse exists {join_type}"
            );
        }
    }

    #[test]
    fn inverse_join_type_maps_the_pairs_and_rejects_the_rest() {
        assert_eq!(inverse_join_type(1), 2);
        assert_eq!(inverse_join_type(2), 1);
        assert_eq!(inverse_join_type(3), 3);
        assert_eq!(inverse_join_type(4), 4);
        assert_eq!(inverse_join_type(5), 9);
        assert_eq!(inverse_join_type(9), 5);
        assert_eq!(inverse_join_type(6), 10);
        assert_eq!(inverse_join_type(10), 6);
        assert_eq!(inverse_join_type(7), -1);
        assert_eq!(inverse_join_type(8), -1);
        assert_eq!(inverse_join_type(0), -1);
    }

    #[test]
    fn every_join_type_label_matches_enum_util() {
        check_labels(
            join_type_to_string,
            &[
                (0, "INVALID"),
                (1, "LEFT"),
                (2, "RIGHT"),
                (3, "INNER"),
                (4, "FULL"),
                (5, "SEMI"),
                (6, "ANTI"),
                (7, "MARK"),
                (8, "SINGLE"),
                (9, "RIGHT_SEMI"),
                (10, "RIGHT_ANTI"),
            ],
        );
        // The empty string is what makes the C++ shell throw the
        // NotImplementedException EnumUtil's `ToChars` would have thrown.
        assert_eq!(join_type_to_string(11), "");
        assert_eq!(join_type_to_string(255), "");
    }

    //===--------------------------------------------------------------------===//
    // Operator type names
    //===--------------------------------------------------------------------===//

    #[test]
    fn every_logical_operator_label_matches_the_cpp() {
        check_labels(
            logical_operator_to_string,
            &[
                (1, "PROJECTION"),
                (2, "FILTER"),
                (3, "AGGREGATE"),
                (4, "WINDOW"),
                (5, "UNNEST"),
                (6, "LIMIT"),
                (7, "ORDER_BY"),
                (8, "TOP_N"),
                (10, "COPY_TO_FILE"),
                (11, "DISTINCT"),
                (12, "SAMPLE"),
                (14, "PIVOT"),
                (15, "COPY_DATABASE"),
                (16, "SECURE_VIEW"),
                (25, "GET"),
                (26, "CHUNK_GET"),
                (27, "DELIM_GET"),
                (28, "EXPRESSION_GET"),
                (29, "DUMMY_SCAN"),
                (30, "EMPTY_RESULT"),
                (31, "CTE_SCAN"),
                (50, "JOIN"),
                (51, "DELIM_JOIN"),
                (52, "COMPARISON_JOIN"),
                (53, "ANY_JOIN"),
                (54, "CROSS_PRODUCT"),
                (55, "POSITIONAL_JOIN"),
                (56, "ASOF_JOIN"),
                (57, "DEPENDENT_JOIN"),
                (75, "UNION"),
                (76, "EXCEPT"),
                (77, "INTERSECT"),
                (78, "REC_CTE"),
                (79, "CTE"),
                (100, "INSERT"),
                (101, "DELETE"),
                (102, "UPDATE"),
                (103, "MERGE_INTO"),
                (104, "TRIGGER"),
                (125, "ALTER"),
                (126, "CREATE_TABLE"),
                (127, "CREATE_INDEX"),
                (128, "CREATE_SEQUENCE"),
                (129, "CREATE_VIEW"),
                (130, "CREATE_SCHEMA"),
                (131, "CREATE_MACRO"),
                (132, "DROP"),
                (133, "PRAGMA"),
                (134, "TRANSACTION"),
                (135, "CREATE_TYPE"),
                (136, "ATTACH"),
                (137, "DETACH"),
                (138, "CREATE_TRIGGER"),
                (150, "EXPLAIN"),
                (175, "PREPARE"),
                (176, "EXECUTE"),
                (177, "EXPORT"),
                (178, "VACUUM"),
                (179, "SET"),
                (180, "LOAD"),
                (181, "RESET"),
                (182, "UPDATE_EXTENSIONS"),
                (183, "CONNECT"),
                (184, "DISCONNECT"),
                (185, "EXTERNAL_RESOURCE"),
                (190, "CREATE_SECRET"),
                (255, "CUSTOM_OP"),
            ],
        );
        assert_eq!(logical_operator_to_string(0), "INVALID");
        assert_eq!(logical_operator_to_string(254), "INVALID");
    }

    #[test]
    fn every_physical_operator_label_matches_the_cpp() {
        check_labels(
            physical_operator_to_string,
            &[
                (1, "ORDER_BY"),
                (2, "LIMIT"),
                (3, "STREAMING_LIMIT"),
                (4, "LIMIT_PERCENT"),
                (5, "TOP_N"),
                (6, "WINDOW"),
                (7, "UNNEST"),
                (8, "UNGROUPED_AGGREGATE"),
                (9, "HASH_GROUP_BY"),
                (10, "PERFECT_HASH_GROUP_BY"),
                (11, "PARTITIONED_AGGREGATE"),
                (12, "LIMITED_DISTINCT"),
                (13, "FILTER"),
                (14, "PROJECTION"),
                (15, "COPY_TO_FILE"),
                (16, "BATCH_COPY_TO_FILE"),
                (17, "RESERVOIR_SAMPLE"),
                (18, "STREAMING_SAMPLE"),
                (19, "STREAMING_WINDOW"),
                (20, "PIVOT"),
                (21, "COPY_DATABASE"),
                (22, "TABLE_SCAN"),
                (23, "DUMMY_SCAN"),
                (24, "COLUMN_DATA_SCAN"),
                (25, "CHUNK_SCAN"),
                (26, "REC_CTE_SCAN"),
                (27, "REC_REC_CTE_SCAN"),
                (28, "CTE_SCAN"),
                (29, "DELIM_SCAN"),
                (30, "EXPRESSION_SCAN"),
                (31, "POSITIONAL_SCAN"),
                (32, "BLOCKWISE_NL_JOIN"),
                (33, "NESTED_LOOP_JOIN"),
                (34, "HASH_JOIN"),
                (35, "CROSS_PRODUCT"),
                (36, "PIECEWISE_MERGE_JOIN"),
                (37, "IE_JOIN"),
                (38, "LEFT_DELIM_JOIN"),
                (39, "RIGHT_DELIM_JOIN"),
                (40, "POSITIONAL_JOIN"),
                (41, "ASOF_JOIN"),
                (42, "UNION"),
                (43, "REC_CTE"),
                (44, "REC_KEY_CTE"),
                (45, "CTE"),
                (46, "INSERT"),
                (47, "BATCH_INSERT"),
                (48, "DELETE"),
                (49, "UPDATE"),
                (50, "MERGE_INTO"),
                (51, "CREATE_TABLE"),
                (52, "CREATE_TABLE_AS"),
                (53, "BATCH_CREATE_TABLE_AS"),
                (54, "CREATE_INDEX"),
                (55, "ALTER"),
                (56, "CREATE_SEQUENCE"),
                (57, "CREATE_VIEW"),
                (58, "CREATE_SCHEMA"),
                (59, "CREATE_MACRO"),
                (60, "DROP"),
                (61, "PRAGMA"),
                (62, "TRANSACTION"),
                (63, "CREATE_TYPE"),
                (64, "CREATE_TRIGGER"),
                (65, "ATTACH"),
                (66, "DETACH"),
                (67, "EXPLAIN"),
                (68, "EXPLAIN_ANALYZE"),
                (69, "EMPTY_RESULT"),
                (70, "EXECUTE"),
                (71, "PREPARE"),
                (72, "VACUUM"),
                (73, "EXPORT"),
                (74, "SET"),
                (75, "SET_VARIABLE"),
                (76, "LOAD"),
                (77, "INOUT_FUNCTION"),
                (78, "RESULT_COLLECTOR"),
                (79, "RESET"),
                (80, "EXTENSION"),
                (81, "VERIFY_VECTOR"),
                (82, "UPDATE_EXTENSIONS"),
                (83, "CONNECT"),
                (84, "DISCONNECT"),
                (85, "EXTERNAL_RESOURCE"),
                (86, "CREATE_SECRET"),
                (87, "RECURSIVE_KEY_JOIN"),
                (88, "SECURE_VIEW"),
                (89, "MERGE_ACTION_SOURCE"),
            ],
        );
        assert_eq!(physical_operator_to_string(0), "INVALID");
        assert_eq!(physical_operator_to_string(255), "INVALID");
    }

    #[test]
    fn every_relation_type_label_matches_the_cpp() {
        check_labels(
            relation_type_to_string,
            &[
                (1, "TABLE_RELATION"),
                (2, "PROJECTION_RELATION"),
                (3, "FILTER_RELATION"),
                (4, "EXPLAIN_RELATION"),
                (5, "CROSS_PRODUCT_RELATION"),
                (6, "JOIN_RELATION"),
                (7, "AGGREGATE_RELATION"),
                (8, "SET_OPERATION_RELATION"),
                (9, "DISTINCT_RELATION"),
                (10, "LIMIT_RELATION"),
                (11, "ORDER_RELATION"),
                (12, "CREATE_VIEW_RELATION"),
                (13, "CREATE_TABLE_RELATION"),
                (14, "INSERT_RELATION"),
                (15, "VALUE_LIST_RELATION"),
                (16, "MATERIALIZED_RELATION"),
                (17, "DELETE_RELATION"),
                (18, "UPDATE_RELATION"),
                (19, "WRITE_CSV_RELATION"),
                (20, "WRITE_PARQUET_RELATION"),
                (21, "READ_CSV_RELATION"),
                (22, "SUBQUERY_RELATION"),
                (23, "TABLE_FUNCTION_RELATION"),
                (24, "VIEW_RELATION"),
                (25, "QUERY_RELATION"),
                (26, "DELIM_JOIN_RELATION"),
                (27, "DELIM_GET_RELATION"),
                (255, "EXTENSION_RELATION"),
            ],
        );
        assert_eq!(relation_type_to_string(0), "INVALID_RELATION");
        assert_eq!(relation_type_to_string(28), "INVALID_RELATION");
    }

    #[test]
    fn every_statement_type_label_matches_the_cpp() {
        check_labels(
            statement_type_to_string,
            &[
                (1, "SELECT"),
                (2, "INSERT"),
                (3, "UPDATE"),
                (4, "CREATE"),
                (5, "DELETE"),
                (6, "PREPARE"),
                (7, "EXECUTE"),
                (8, "ALTER"),
                (9, "TRANSACTION"),
                (10, "COPY"),
                (11, "ANALYZE"),
                (12, "VARIABLE_SET"),
                (13, "CREATE_FUNC"),
                (14, "EXPLAIN"),
                (15, "DROP"),
                (16, "EXPORT"),
                (17, "PRAGMA"),
                (18, "VACUUM"),
                (19, "CALL"),
                (20, "SET"),
                (21, "LOAD"),
                (22, "RELATION"),
                (23, "EXTENSION"),
                (24, "LOGICAL_PLAN"),
                (25, "ATTACH"),
                (26, "DETACH"),
                (27, "MULTI"),
                (28, "COPY_DATABASE"),
                (29, "UPDATE_EXTENSIONS"),
                (30, "MERGE_INTO"),
                (31, "CONNECT"),
                (32, "DISCONNECT"),
                (33, "EXTERNAL_RESOURCE"),
            ],
        );
        assert_eq!(statement_type_to_string(0), "INVALID");
        assert_eq!(statement_type_to_string(34), "INVALID");
    }

    #[test]
    fn every_statement_return_type_label_matches_the_cpp() {
        check_labels(
            statement_return_type_to_string,
            &[(0, "QUERY_RESULT"), (1, "CHANGED_ROWS"), (2, "NOTHING")],
        );
        assert_eq!(statement_return_type_to_string(3), "INVALID");
    }

    //===--------------------------------------------------------------------===//
    // OptimizerType
    //===--------------------------------------------------------------------===//

    #[test]
    fn the_optimizer_table_is_self_consistent() {
        let mut names = HashSet::new();
        let mut discriminants = HashSet::new();
        for &(name, discriminant) in OPTIMIZER_TABLE {
            assert!(!name.is_empty());
            assert!(names.insert(name), "duplicate optimizer name {name}");
            assert!(
                discriminants.insert(discriminant),
                "duplicate optimizer discriminant {discriminant}"
            );
        }
        assert_eq!(optimizer_count(), OPTIMIZER_TABLE.len());
        assert_eq!(optimizer_count(), 45);
    }

    #[test]
    fn optimizer_lookups_are_inverses() {
        for &(name, discriminant) in OPTIMIZER_TABLE {
            assert_eq!(optimizer_name_for_discriminant(discriminant), name);
            assert_eq!(
                optimizer_discriminant_of_name(name.as_bytes()),
                discriminant as i64
            );
        }
        assert_eq!(optimizer_name_for_discriminant(0), "");
        assert_eq!(optimizer_name_for_discriminant(9999), "");
        assert_eq!(optimizer_discriminant_of_name(b"nonsense"), -1);
        assert_eq!(optimizer_discriminant_of_name(b"expression_rewriter"), 1);
    }

    #[test]
    fn optimizer_names_are_addressable_by_index() {
        for (index, &(name, _)) in OPTIMIZER_TABLE.iter().enumerate() {
            assert_eq!(optimizer_name(index), name);
        }
        assert_eq!(optimizer_name(OPTIMIZER_TABLE.len()), "");
    }
}
