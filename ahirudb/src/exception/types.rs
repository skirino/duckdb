//! Exception types and the types that marshal them across the Crubit boundary.
//!
//! Mirrors `ExceptionType`, `EXCEPTION_MAP` and `QueryLocation` from
//! `src/common/exception.cpp` and `src/include/duckdb/common/exception.hpp`.
//! Everything that is not an FFI entry point is crate-private: only what C++
//! has to name is `pub`, so that the generated bindings stay a list of the seam
//! rather than of the whole module.

/// Exception types matching the C++ `ExceptionType` enum exactly. Keep in sync
/// with `duckdb::ExceptionType` in `src/include/duckdb/common/exception.hpp`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ExceptionType {
    INVALID = 0,
    OUT_OF_RANGE = 1,
    CONVERSION = 2,
    UNKNOWN_TYPE = 3,
    DECIMAL = 4,
    MISMATCH_TYPE = 5,
    DIVIDE_BY_ZERO = 6,
    OBJECT_SIZE = 7,
    INVALID_TYPE = 8,
    SERIALIZATION = 9,
    TRANSACTION = 10,
    NOT_IMPLEMENTED = 11,
    EXPRESSION = 12,
    CATALOG = 13,
    PARSER = 14,
    PLANNER = 15,
    SCHEDULER = 16,
    EXECUTOR = 17,
    CONSTRAINT = 18,
    INDEX = 19,
    STAT = 20,
    CONNECTION = 21,
    SYNTAX = 22,
    SETTINGS = 23,
    BINDER = 24,
    NETWORK = 25,
    OPTIMIZER = 26,
    NULL_POINTER = 27,
    IO = 28,
    INTERRUPT = 29,
    FATAL = 30,
    INTERNAL = 31,
    INVALID_INPUT = 32,
    OUT_OF_MEMORY = 33,
    PERMISSION = 34,
    PARAMETER_NOT_RESOLVED = 35,
    PARAMETER_NOT_ALLOWED = 36,
    DEPENDENCY = 37,
    HTTP = 38,
    MISSING_EXTENSION = 39,
    AUTOLOAD = 40,
    SEQUENCE = 41,
    INVALID_CONFIGURATION = 42,
    DATA_CORRUPTION = 43,
    RESOURCE_IN_USE = 44,
}

impl ExceptionType {
    /// Total number of exception type variants.
    pub(crate) const COUNT: usize = 45;

    /// The type with the given discriminant, or `None` when there is no such
    /// type.
    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        Some(match value
        {
            0 => Self::INVALID,
            1 => Self::OUT_OF_RANGE,
            2 => Self::CONVERSION,
            3 => Self::UNKNOWN_TYPE,
            4 => Self::DECIMAL,
            5 => Self::MISMATCH_TYPE,
            6 => Self::DIVIDE_BY_ZERO,
            7 => Self::OBJECT_SIZE,
            8 => Self::INVALID_TYPE,
            9 => Self::SERIALIZATION,
            10 => Self::TRANSACTION,
            11 => Self::NOT_IMPLEMENTED,
            12 => Self::EXPRESSION,
            13 => Self::CATALOG,
            14 => Self::PARSER,
            15 => Self::PLANNER,
            16 => Self::SCHEDULER,
            17 => Self::EXECUTOR,
            18 => Self::CONSTRAINT,
            19 => Self::INDEX,
            20 => Self::STAT,
            21 => Self::CONNECTION,
            22 => Self::SYNTAX,
            23 => Self::SETTINGS,
            24 => Self::BINDER,
            25 => Self::NETWORK,
            26 => Self::OPTIMIZER,
            27 => Self::NULL_POINTER,
            28 => Self::IO,
            29 => Self::INTERRUPT,
            30 => Self::FATAL,
            31 => Self::INTERNAL,
            32 => Self::INVALID_INPUT,
            33 => Self::OUT_OF_MEMORY,
            34 => Self::PERMISSION,
            35 => Self::PARAMETER_NOT_RESOLVED,
            36 => Self::PARAMETER_NOT_ALLOWED,
            37 => Self::DEPENDENCY,
            38 => Self::HTTP,
            39 => Self::MISSING_EXTENSION,
            40 => Self::AUTOLOAD,
            41 => Self::SEQUENCE,
            42 => Self::INVALID_CONFIGURATION,
            43 => Self::DATA_CORRUPTION,
            44 => Self::RESOURCE_IN_USE,
            value =>
            {
                debug_assert!(
                    value as usize >= Self::COUNT,
                    "the match above covers every variant"
                );
                return None;
            },
        })
    }
}

/// The `EXCEPTION_MAP` of `src/common/exception.cpp`: the name every exception
/// type is reported under. The names are part of the error text clients see
/// (`ErrorData::ConstructFinalMessage` prefixes them, and the JSON payload
/// carries them), so they are reproduced verbatim.
///
/// `NETWORK` is deliberately absent, exactly as in the C++ map, which is what
/// makes [`exception_type_to_string`] answer `"Unknown"` for it.
const EXCEPTION_MAP: &[(ExceptionType, &str)] = &[
    (ExceptionType::INVALID, "Invalid"),
    (ExceptionType::OUT_OF_RANGE, "Out of Range"),
    (ExceptionType::CONVERSION, "Conversion"),
    (ExceptionType::UNKNOWN_TYPE, "Unknown Type"),
    (ExceptionType::DECIMAL, "Decimal"),
    (ExceptionType::MISMATCH_TYPE, "Mismatch Type"),
    (ExceptionType::DIVIDE_BY_ZERO, "Divide by Zero"),
    (ExceptionType::OBJECT_SIZE, "Object Size"),
    (ExceptionType::INVALID_TYPE, "Invalid type"),
    (ExceptionType::SERIALIZATION, "Serialization"),
    (ExceptionType::TRANSACTION, "TransactionContext"),
    (ExceptionType::NOT_IMPLEMENTED, "Not implemented"),
    (ExceptionType::EXPRESSION, "Expression"),
    (ExceptionType::CATALOG, "Catalog"),
    (ExceptionType::PARSER, "Parser"),
    (ExceptionType::BINDER, "Binder"),
    (ExceptionType::PLANNER, "Planner"),
    (ExceptionType::SCHEDULER, "Scheduler"),
    (ExceptionType::EXECUTOR, "Executor"),
    (ExceptionType::CONSTRAINT, "Constraint"),
    (ExceptionType::INDEX, "Index"),
    (ExceptionType::STAT, "Stat"),
    (ExceptionType::CONNECTION, "Connection"),
    (ExceptionType::SYNTAX, "Syntax"),
    (ExceptionType::SETTINGS, "Settings"),
    (ExceptionType::OPTIMIZER, "Optimizer"),
    (ExceptionType::NULL_POINTER, "NullPointer"),
    (ExceptionType::IO, "IO"),
    (ExceptionType::INTERRUPT, "INTERRUPT"),
    (ExceptionType::FATAL, "FATAL"),
    (ExceptionType::INTERNAL, "INTERNAL"),
    (ExceptionType::INVALID_INPUT, "Invalid Input"),
    (ExceptionType::OUT_OF_MEMORY, "Out of Memory"),
    (ExceptionType::PERMISSION, "Permission"),
    (ExceptionType::PARAMETER_NOT_RESOLVED, "Parameter Not Resolved"),
    (ExceptionType::PARAMETER_NOT_ALLOWED, "Parameter Not Allowed"),
    (ExceptionType::DEPENDENCY, "Dependency"),
    (ExceptionType::MISSING_EXTENSION, "Missing Extension"),
    (ExceptionType::HTTP, "HTTP"),
    (ExceptionType::AUTOLOAD, "Extension Autoloading"),
    (ExceptionType::SEQUENCE, "Sequence"),
    (ExceptionType::INVALID_CONFIGURATION, "Invalid Configuration"),
    (ExceptionType::DATA_CORRUPTION, "Data Corruption"),
    (ExceptionType::RESOURCE_IN_USE, "Resource In Use"),
];

/// The name an exception type is reported under, or `"Unknown"` when it has
/// none.
pub(crate) fn exception_type_to_string(exception_type: u8) -> &'static str {
    let Some(exception_type) = ExceptionType::from_u8(exception_type)
    else
    {
        return "Unknown";
    };
    EXCEPTION_MAP
        .iter()
        .find(|(entry_type, _)| *entry_type == exception_type)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

/// The exception type a name stands for, or `None` when it names none.
pub(crate) fn string_to_exception_type(name: &[u8]) -> Option<ExceptionType> {
    EXCEPTION_MAP
        .iter()
        .find(|(_, entry_name)| entry_name.as_bytes() == name)
        .map(|(exception_type, _)| *exception_type)
}

/// A query location, mirroring `duckdb::QueryLocation`. An invalid location has
/// `INVALID_OFFSET` as its offset and stands for "no known location".
///
/// Deliberately without a `Default`: the C++ default of the offset is "not
/// known", which is not what a zeroed `offset` field would mean.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct QueryLocationLayout {
    pub offset: u32,
    pub length: u32,
}

impl QueryLocationLayout {
    /// `duckdb::QueryLocation::INVALID_OFFSET`, the offset that marks a
    /// location as unknown.
    pub const INVALID_OFFSET: u32 = u32::MAX;

    /// Only the tests build one of these; C++ hands them over field by field.
    #[cfg(test)]
    pub(crate) fn new(offset: u32, length: u32) -> Self { Self { offset, length } }

    pub(crate) fn invalid() -> Self { Self { offset: Self::INVALID_OFFSET, length: 0 } }

    pub(crate) fn is_valid(&self) -> bool { self.offset != Self::INVALID_OFFSET }
}

/// A borrowed run of bytes. DuckDB strings are not necessarily valid UTF-8, so
/// everything crossing the boundary is carried as bytes rather than as text.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ByteSlice {
    pub ptr: *const u8,
    pub len: usize,
}

impl ByteSlice {
    /// Only the tests build one of these; C++ fills the fields in directly,
    /// which is the whole point of the type being a plain aggregate.
    #[cfg(test)]
    pub(crate) fn new(value: &[u8]) -> Self { Self { ptr: value.as_ptr(), len: value.len() } }

    /// Borrows a C++ string. The bytes stay alive as long as the C++ string
    /// they came from does, which for every caller is the length of the FFI
    /// call.
    pub(crate) fn as_slice(&self) -> &[u8] {
        if self.len == 0
        {
            return &[];
        }
        // SAFETY: a `ByteSlice` is only ever built - on either side of the
        // boundary - from a live `[u8]`, and borrowing through `&self`
        // keeps that borrow alive.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// One `extra_info` entry, as an exception hands it to C++.
#[repr(C)]
pub struct ExtraInfoEntry {
    pub key: OwnedBytes,
    pub value: OwnedBytes,
}

/// What the exception factories hand back: the message they built, plus the
/// `extra_info` entries C++ should put around it.
///
/// The message is empty for the entry points that build `extra_info` only. C++
/// then builds the `Exception` from it, which is what turns the pair into the
/// JSON payload `what()` carries: Rust formats the text, C++ owns the
/// exception.
///
/// Every allocation in here belongs to Rust and is released by
/// [`crate::exception::bridge::free_payload`], exactly once.
#[repr(C)]
pub struct ExceptionPayload {
    pub message: OwnedBytes,
    pub entry_count: usize,
    pub entries: *mut ExtraInfoEntry,
}

/// A run of bytes Rust owns. C++ copies them out and hands the value back to
/// [`crate::exception::bridge::free_payload`] exactly once, which frees them
/// again.
#[repr(C)]
pub struct OwnedBytes {
    pub ptr: *mut u8,
    pub len: usize,
}

impl OwnedBytes {
    /// Copies `value` into a fresh allocation.
    pub(crate) fn from_vec(value: Vec<u8>) -> Self {
        // `into_boxed_slice` shrinks the allocation to exactly the length,
        // which is what lets `into_vec` hand the same shape back to the
        // allocator.
        let value = value.into_boxed_slice();
        let len = value.len();
        #[cfg(test)]
        LIVE_PAYLOADS.with(|live| live.set(live.get() + 1));
        Self { ptr: Box::into_raw(value) as *mut u8, len }
    }

    /// The bytes. They stay alive as long as the payload this field belongs to,
    /// which is what ties the returned slice to `&self`. Only the tests
    /// read them: C++ reads `ptr` and `len` off the struct.
    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[u8] {
        if self.len == 0
        {
            return &[];
        }
        // SAFETY: an `OwnedBytes` is only ever built by `from_vec`, which hands
        // over the bytes it keeps alive until `free_payload`; borrowing
        // through `&self` keeps the payload, and so the allocation,
        // alive.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Gives the bytes back. Paired with [`Self::from_vec`].
    ///
    /// # Safety
    /// `self` must be a value from [`Self::from_vec`] whose allocation is still
    /// alive.
    pub(crate) unsafe fn into_vec(self) -> Vec<u8> {
        #[cfg(test)]
        LIVE_PAYLOADS.with(|live| live.set(live.get() - 1));
        // SAFETY: `from_vec` handed over a `Box<[u8]>` of this length, and
        // `Vec::from` takes that box back without reallocating, so the
        // pointer and the layout the allocator is asked to free are the
        // ones it handed out.
        unsafe { Vec::from(Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.ptr, self.len))) }
    }
}

/// How many payloads the thread has handed out and not taken back. Only a test
/// reads this: it is here so that a payload leak - or a double free, which
/// drives the count negative - shows up as a failing test rather than much
/// later, in a query. It is per thread because a payload is built and released
/// by the thread that calls into Rust.
#[cfg(test)]
pub(crate) fn live_payloads() -> usize { LIVE_PAYLOADS.with(|live| live.get()) }

#[cfg(test)]
thread_local! {
    static LIVE_PAYLOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The `extra_info` entries of an exception, in the order they are built.
/// DuckDB's `extra_info` is an `unordered_map`, so the order only has to be
/// deterministic, not meaningful.
pub(crate) type ExtraInfo = Vec<(Vec<u8>, Vec<u8>)>;

/// Moves the built entries into a payload the C++ side can free.
pub(crate) fn into_payload(message: Vec<u8>, extra_info: ExtraInfo) -> ExceptionPayload {
    let entries: Box<[ExtraInfoEntry]> = extra_info
        .into_iter()
        .map(|(key, value)| ExtraInfoEntry {
            key: OwnedBytes::from_vec(key),
            value: OwnedBytes::from_vec(value),
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let entry_count = entries.len();
    ExceptionPayload {
        message: OwnedBytes::from_vec(message),
        entry_count,
        // The box is not dropped here; `free_payload` rebuilds it from this pointer and
        // the same length.
        entries: Box::into_raw(entries) as *mut ExtraInfoEntry,
    }
}

/// Releases everything a payload owns. Paired with [`into_payload`].
///
/// Each part is taken back on its own: `OwnedBytes` and `ExtraInfoEntry` are
/// raw pointers rather than something with a `Drop`, so dropping the box frees
/// the box and nothing it points at.
pub(crate) fn free_payload(payload: ExceptionPayload) {
    // SAFETY: every allocation in a payload is made by `into_payload`, and this
    // is the only owner of them once it returns, so each is freed here
    // exactly once. `Box::into_raw` hands out a dangling but aligned
    // pointer for an empty box, which is what `from_raw_parts_mut` takes
    // for a length of zero.
    unsafe {
        drop(payload.message.into_vec());
        for entry in std::slice::from_raw_parts_mut(payload.entries, payload.entry_count)
        {
            drop(std::ptr::read(&entry.key).into_vec());
            drop(std::ptr::read(&entry.value).into_vec());
        }
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            payload.entries,
            payload.entry_count,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_type_discriminants_round_trip() {
        for i in 0..ExceptionType::COUNT as u8
        {
            assert_eq!(Some(i), ExceptionType::from_u8(i).map(|t| t as u8));
        }
        assert_eq!(None, ExceptionType::from_u8(ExceptionType::COUNT as u8));
        assert_eq!(None, ExceptionType::from_u8(255));
    }

    #[test]
    fn names_are_the_ones_the_error_text_uses() {
        assert_eq!("Binder", exception_type_to_string(24));
        assert_eq!("Catalog", exception_type_to_string(13));
        assert_eq!("Parser", exception_type_to_string(14));
        assert_eq!("Conversion", exception_type_to_string(2));
        assert_eq!("Invalid Input", exception_type_to_string(32));
        // The one name that is not the title-cased enum variant
        assert_eq!("Invalid type", exception_type_to_string(8));
        assert_eq!("TransactionContext", exception_type_to_string(10));
        assert_eq!("Extension Autoloading", exception_type_to_string(40));
    }

    #[test]
    fn network_has_no_name() {
        // NETWORK was left out of the C++ map, and its name is part of the
        // error text.
        assert_eq!("Unknown", exception_type_to_string(25));
    }

    #[test]
    fn an_unknown_discriminant_has_no_name() {
        assert_eq!("Unknown", exception_type_to_string(45));
        assert_eq!("Unknown", exception_type_to_string(255));
    }

    #[test]
    fn names_round_trip_back_to_types() {
        for (exception_type, name) in EXCEPTION_MAP
        {
            assert_eq!(
                Some(*exception_type),
                string_to_exception_type(name.as_bytes()),
                "{name} did not round trip"
            );
        }
    }

    #[test]
    fn a_name_that_is_in_no_map_entry_is_invalid() {
        assert_eq!(None, string_to_exception_type(b""));
        assert_eq!(None, string_to_exception_type(b"Binder Error"));
        assert_eq!(None, string_to_exception_type(b"binder"));
        assert_eq!(None, string_to_exception_type("Binder\u{FFFD}".as_bytes()));
    }

    #[test]
    fn every_name_is_distinct() {
        let mut names: Vec<&str> = EXCEPTION_MAP.iter().map(|(_, name)| *name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(count, names.len());
    }

    #[test]
    fn every_type_but_network_has_a_name() {
        for i in 0..ExceptionType::COUNT as u8
        {
            if i == ExceptionType::NETWORK as u8
            {
                continue;
            }
            assert_ne!("Unknown", exception_type_to_string(i), "type {i} lost its name");
        }
    }

    #[test]
    fn query_locations_know_whether_they_are_known() {
        assert!(QueryLocationLayout::new(4, 2).is_valid());
        assert!(QueryLocationLayout::new(0, 0).is_valid());
        assert!(!QueryLocationLayout::invalid().is_valid());
    }

    #[test]
    fn byte_slices_borrow_what_they_were_built_from() {
        let value = b"hello";
        let slice = ByteSlice::new(value);
        assert_eq!(b"hello", slice.as_slice());
        // An empty slice never dereferences its pointer, so the borrow is still
        // sound.
        let empty = ByteSlice::new(&[]);
        assert_eq!(0, empty.as_slice().len());
    }

    #[test]
    fn owned_bytes_hand_the_allocation_back() {
        let owned = OwnedBytes::from_vec(b"message".to_vec());
        assert_eq!(b"message", owned.as_slice());
        // SAFETY: `owned` came from `from_vec` and has not been freed
        let recovered = unsafe { owned.into_vec() };
        assert_eq!(b"message", recovered.as_slice());
    }

    #[test]
    fn an_empty_payload_is_still_released() {
        let payload = into_payload(Vec::new(), Vec::new());
        assert_eq!(0, payload.entry_count);
        assert!(payload.message.as_slice().is_empty());
        free_payload(payload);
    }
}
