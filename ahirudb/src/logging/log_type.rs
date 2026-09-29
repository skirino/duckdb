#![allow(non_camel_case_types)]
#![allow(nonstandard_style)]

use super::LogLevel;
use std::collections::HashMap;

// Placeholder for DuckDB's LogicalType - will be replaced with actual type when porting types module
#[derive(Debug, Clone, PartialEq)]
pub enum LogicalType {
    VARCHAR,
    BIGINT,
    UBIGINT,
    TIMESTAMP_TZ,
    STRUCT(Vec<(String, LogicalType)>),
    MAP(Box<LogicalType>, Box<LogicalType>),
    LIST(Box<LogicalType>),
}

impl LogicalType {
    pub fn is_nested(&self) -> bool {
        matches!(
            self,
            LogicalType::STRUCT(_) | LogicalType::MAP(_, _) | LogicalType::LIST(_)
        )
    }
}

// Type aliases for external types (will be replaced when those modules are ported)
pub type FileHandle = u64;
pub type BaseRequest = u64;
pub type HTTPResponse = u64;
pub type PhysicalOperator = u64;
pub type PhysicalOperatorType = u8;
pub type AttachedDatabase = u64;
pub type RowGroup = u64;
pub type DataTableInfo = u64;
pub type Value = String; // Placeholder
pub type transaction_t = u64;
pub const MAX_TRANSACTION_ID: transaction_t = transaction_t::MAX;

#[derive(Debug, Clone)]
pub struct LogType {
    pub name: String,
    pub level: LogLevel,
    pub is_structured: bool,
    pub typ: LogicalType,
}

impl LogType {
    pub fn new_unstructured(name: String, level: LogLevel) -> Self {
        Self {
            name,
            level,
            is_structured: false,
            typ: LogicalType::VARCHAR,
        }
    }

    pub fn new_structured(name: String, level: LogLevel, typ: LogicalType) -> Self {
        if !typ.is_nested() {
            panic!("LogType must be nested if the type is explicitly set");
        }
        Self {
            name,
            level,
            is_structured: true,
            typ,
        }
    }
}

// DefaultLogType
#[derive(Debug, Clone)]
pub struct DefaultLogType {
    pub base: LogType,
}

impl DefaultLogType {
    pub const NAME: &'static str = "";
    pub const LEVEL: LogLevel = LogLevel::LOG_INFO;

    pub fn new() -> Self {
        Self {
            base: LogType::new_unstructured(Self::NAME.to_string(), Self::LEVEL),
        }
    }
}

// QueryLogType
#[derive(Debug, Clone)]
pub struct QueryLogType {
    pub base: LogType,
}

impl QueryLogType {
    pub const NAME: &'static str = "QueryLog";
    pub const LEVEL: LogLevel = LogLevel::LOG_INFO;

    pub fn new() -> Self {
        Self {
            base: LogType::new_unstructured(Self::NAME.to_string(), Self::LEVEL),
        }
    }

    pub fn construct_log_message(str: &str) -> String {
        format!("QUERY: {}", str)
    }
}

// FileSystemLogType
#[derive(Debug, Clone)]
pub struct FileSystemLogType {
    pub base: LogType,
}

impl FileSystemLogType {
    pub const NAME: &'static str = "FileSystem";
    pub const LEVEL: LogLevel = LogLevel::LOG_TRACE;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("fs".to_string(), LogicalType::VARCHAR),
            ("path".to_string(), LogicalType::VARCHAR),
            ("op".to_string(), LogicalType::VARCHAR),
            ("bytes".to_string(), LogicalType::BIGINT),
            ("pos".to_string(), LogicalType::BIGINT),
        ])
    }

    pub fn construct_log_message(handle: &FileHandle, op: &str, bytes: i64, pos: usize) -> String {
        format!(
            "FILE_SYSTEM: handle={}, op={}, bytes={}, pos={}",
            handle, op, bytes, pos
        )
    }

    pub fn construct_log_message_simple(handle: &FileHandle, op: &str) -> String {
        format!("FILE_SYSTEM: handle={}, op={}", handle, op)
    }
}

// HTTPLogType
#[derive(Debug, Clone)]
pub struct HTTPLogType {
    pub base: LogType,
}

impl HTTPLogType {
    pub const NAME: &'static str = "HTTP";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("method".to_string(), LogicalType::VARCHAR),
            ("url".to_string(), LogicalType::VARCHAR),
            ("status".to_string(), LogicalType::BIGINT),
            ("latency_ms".to_string(), LogicalType::BIGINT),
        ])
    }

    pub fn construct_log_message(request: &BaseRequest, response: Option<&HTTPResponse>) -> String {
        format!("HTTP: request={}, response={:?}", request, response)
    }

    pub fn construct_log_message_simple(str: &str) -> String {
        str.to_string()
    }
}

// PhysicalOperatorLogType
#[derive(Debug, Clone)]
pub struct PhysicalOperatorLogType {
    pub base: LogType,
}

impl PhysicalOperatorLogType {
    pub const NAME: &'static str = "PhysicalOperator";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("operator_type".to_string(), LogicalType::VARCHAR),
            ("event".to_string(), LogicalType::VARCHAR),
            (
                "info".to_string(),
                LogicalType::MAP(
                    Box::new(LogicalType::VARCHAR),
                    Box::new(LogicalType::VARCHAR),
                ),
            ),
        ])
    }

    pub fn construct_log_message(
        _op: &PhysicalOperator,
        class_p: &str,
        event: &str,
        info: &[(String, String)],
    ) -> String {
        let info_map: HashMap<_, _> = info.iter().cloned().collect();
        format!(
            "PHYSICAL_OPERATOR: class={}, event={}, info={:?}",
            class_p, event, info_map
        )
    }

    pub fn construct_log_message_alt(
        operator_type: PhysicalOperatorType,
        parameters: &[(String, String)],
        class_p: &str,
        event: &str,
        info: &[(String, String)],
    ) -> String {
        let params_map: HashMap<_, _> = parameters.iter().cloned().collect();
        let info_map: HashMap<_, _> = info.iter().cloned().collect();
        format!(
            "PHYSICAL_OPERATOR: type={}, class={}, event={}, params={:?}, info={:?}",
            operator_type, class_p, event, params_map, info_map
        )
    }
}

// MetricsLogType
#[derive(Debug, Clone)]
pub struct MetricsLogType {
    pub base: LogType,
}

impl MetricsLogType {
    pub const NAME: &'static str = "Metrics";
    pub const LEVEL: LogLevel = LogLevel::LOG_INFO;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("metric".to_string(), LogicalType::VARCHAR),
            ("value".to_string(), LogicalType::VARCHAR), // Value as string for now
        ])
    }

    pub fn construct_log_message(metric: &str, value: &Value) -> String {
        format!("METRICS: metric={}, value={}", metric, value)
    }
}

// CheckpointLogType
#[derive(Debug, Clone)]
pub struct CheckpointLogType {
    pub base: LogType,
}

impl CheckpointLogType {
    pub const NAME: &'static str = "Checkpoint";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("operation".to_string(), LogicalType::VARCHAR), // "VACUUM" or "CHECKPOINT"
            ("database".to_string(), LogicalType::VARCHAR),
            ("table".to_string(), LogicalType::VARCHAR),
            ("segment_idx".to_string(), LogicalType::BIGINT),
            (
                "details".to_string(),
                LogicalType::MAP(
                    Box::new(LogicalType::VARCHAR),
                    Box::new(LogicalType::VARCHAR),
                ),
            ),
        ])
    }

    pub fn construct_log_message_vacuum(
        _db: &AttachedDatabase,
        _table: &DataTableInfo,
        segment_idx: usize,
        merge_count: usize,
        target_count: usize,
        merge_rows: usize,
        row_start: usize,
    ) -> String {
        format!(
            "CHECKPOINT: VACUUM segment_idx={}, merge_count={}, target_count={}, merge_rows={}, row_start={}",
            segment_idx, merge_count, target_count, merge_rows, row_start
        )
    }

    pub fn construct_log_message_checkpoint(
        _db: &AttachedDatabase,
        _table: &DataTableInfo,
        segment_idx: usize,
        _row_group: &RowGroup,
        row_group_start: usize,
    ) -> String {
        format!(
            "CHECKPOINT: CHECKPOINT segment_idx={}, row_group_start={}",
            segment_idx, row_group_start
        )
    }
}

// TransactionLogType
#[derive(Debug, Clone)]
pub struct TransactionLogType {
    pub base: LogType,
}

impl TransactionLogType {
    pub const NAME: &'static str = "Transaction";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("transaction_id".to_string(), LogicalType::BIGINT),
            ("log_type".to_string(), LogicalType::VARCHAR),
            ("database".to_string(), LogicalType::VARCHAR),
        ])
    }

    pub fn construct_log_message(
        _db: &AttachedDatabase,
        log_type: &str,
        transaction_id: transaction_t,
    ) -> String {
        format!(
            "TRANSACTION: type={}, transaction_id={}",
            log_type, transaction_id
        )
    }
}

// AdaptiveFilterLogType
#[derive(Debug, Clone)]
pub struct AdaptiveFilterLogType {
    pub base: LogType,
}

impl AdaptiveFilterLogType {
    pub const NAME: &'static str = "AdaptiveFilter";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("event".to_string(), LogicalType::VARCHAR),
            ("file_path".to_string(), LogicalType::VARCHAR),
            (
                "permutation".to_string(),
                LogicalType::LIST(Box::new(LogicalType::BIGINT)),
            ),
            (
                "info".to_string(),
                LogicalType::MAP(
                    Box::new(LogicalType::VARCHAR),
                    Box::new(LogicalType::VARCHAR),
                ),
            ),
        ])
    }

    pub fn construct_log_message(
        event: &str,
        file_path: &str,
        permutation: &[usize],
        info: &[(String, String)],
    ) -> String {
        let info_map: HashMap<_, _> = info.iter().cloned().collect();
        format!(
            "ADAPTIVE_FILTER: event={}, file_path={}, permutation={:?}, info={:?}",
            event, file_path, permutation, info_map
        )
    }
}

// ParquetPrefetchLogType
#[derive(Debug, Clone)]
pub struct ParquetPrefetchLogType {
    pub base: LogType,
}

impl ParquetPrefetchLogType {
    pub const NAME: &'static str = "ParquetPrefetch";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("file_path".to_string(), LogicalType::VARCHAR),
            ("row_group_id".to_string(), LogicalType::BIGINT),
            ("fully_filtered".to_string(), LogicalType::VARCHAR), // bool as string for now
            ("strategy".to_string(), LogicalType::VARCHAR),
            (
                "prefetch_groups".to_string(),
                LogicalType::LIST(Box::new(LogicalType::LIST(Box::new(LogicalType::VARCHAR)))),
            ),
            (
                "minimal_filters".to_string(),
                LogicalType::LIST(Box::new(LogicalType::VARCHAR)),
            ),
            ("accepted_column_gap".to_string(), LogicalType::BIGINT),
        ])
    }

    pub fn construct_log_message(
        file_path: &str,
        row_group_id: usize,
        fully_filtered: bool,
        strategy: &str,
        prefetch_groups: &[Vec<String>],
        minimal_filters: &[String],
        accepted_column_gap: u64,
    ) -> String {
        format!(
            "PARQUET_PREFETCH: file_path={}, row_group_id={}, fully_filtered={}, strategy={}, prefetch_groups={:?}, minimal_filters={:?}, accepted_column_gap={}",
            file_path,
            row_group_id,
            fully_filtered,
            strategy,
            prefetch_groups,
            minimal_filters,
            accepted_column_gap
        )
    }
}

// AsyncTaskScheduleLogType
#[derive(Debug, Clone)]
pub struct AsyncTaskScheduleLogType {
    pub base: LogType,
}

impl AsyncTaskScheduleLogType {
    pub const NAME: &'static str = "AsyncTaskSchedule";
    pub const LEVEL: LogLevel = LogLevel::LOG_DEBUG;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("pool".to_string(), LogicalType::VARCHAR),
            ("task_count".to_string(), LogicalType::BIGINT),
        ])
    }

    pub fn construct_log_message(pool: &str, task_count: usize) -> String {
        format!(
            "ASYNC_TASK_SCHEDULE: pool={}, task_count={}",
            pool, task_count
        )
    }
}

// ExternalResourceLogType
#[derive(Debug, Clone)]
pub struct ExternalResourceLogType {
    pub base: LogType,
}

impl ExternalResourceLogType {
    pub const NAME: &'static str = "ExternalResource";
    pub const LEVEL: LogLevel = LogLevel::LOG_INFO;

    pub fn new() -> Self {
        Self {
            base: LogType::new_structured(
                Self::NAME.to_string(),
                Self::LEVEL,
                Self::get_log_type(),
            ),
        }
    }

    pub fn get_log_type() -> LogicalType {
        LogicalType::STRUCT(vec![
            ("resource_type".to_string(), LogicalType::VARCHAR),
            ("resource_name".to_string(), LogicalType::VARCHAR),
            ("operation".to_string(), LogicalType::VARCHAR),
            ("error".to_string(), LogicalType::VARCHAR),
            ("extra_info".to_string(), LogicalType::VARCHAR), // Value as string for now
        ])
    }

    pub fn construct_log_message(
        resource_type: &str,
        resource_name: &str,
        operation: &str,
        error: &str,
        extra_info: &Value,
    ) -> String {
        format!(
            "EXTERNAL_RESOURCE: type={}, name={}, operation={}, error={}, extra_info={}",
            resource_type, resource_name, operation, error, extra_info
        )
    }
}

// Registry for all log types
use std::sync::OnceLock;

static LOG_TYPE_REGISTRY: OnceLock<HashMap<String, Box<dyn LogTypeRegistryEntry>>> =
    OnceLock::new();

pub trait LogTypeRegistryEntry: Send + Sync {
    fn name(&self) -> &str;
    fn level(&self) -> LogLevel;
    fn is_structured(&self) -> bool;
    fn typ(&self) -> &LogicalType;
}

impl LogTypeRegistryEntry for DefaultLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        false
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for QueryLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        false
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for FileSystemLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for HTTPLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for PhysicalOperatorLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for MetricsLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for CheckpointLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for TransactionLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for AdaptiveFilterLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for ParquetPrefetchLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for AsyncTaskScheduleLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

impl LogTypeRegistryEntry for ExternalResourceLogType {
    fn name(&self) -> &str {
        Self::NAME
    }
    fn level(&self) -> LogLevel {
        Self::LEVEL
    }
    fn is_structured(&self) -> bool {
        true
    }
    fn typ(&self) -> &LogicalType {
        &self.base.typ
    }
}

pub fn register_default_log_types() {
    let mut registry: HashMap<String, Box<dyn LogTypeRegistryEntry>> = HashMap::new();

    macro_rules! register {
        ($type:ty) => {
            let entry: Box<dyn LogTypeRegistryEntry> = Box::new(<$type>::new());
            registry.insert(entry.name().to_string(), entry);
        };
    }

    register!(DefaultLogType);
    register!(QueryLogType);
    register!(FileSystemLogType);
    register!(HTTPLogType);
    register!(PhysicalOperatorLogType);
    register!(MetricsLogType);
    register!(CheckpointLogType);
    register!(TransactionLogType);
    register!(AdaptiveFilterLogType);
    register!(ParquetPrefetchLogType);
    register!(AsyncTaskScheduleLogType);
    register!(ExternalResourceLogType);

    LOG_TYPE_REGISTRY.set(registry).ok();
}

pub fn lookup_log_type(name: &str) -> Option<&'static dyn LogTypeRegistryEntry> {
    LOG_TYPE_REGISTRY.get()?.get(name).map(|b| b.as_ref())
}
