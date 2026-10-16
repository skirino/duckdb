//===----------------------------------------------------------------------===//
//                         DuckDB
//
// duckdb/common/multi_file/multi_file_data.hpp
//
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/exception/conversion_exception.hpp"
#include "duckdb/common/types/selection_vector.hpp"
#include "duckdb/parser/expression/constant_expression.hpp"
#include "duckdb/common/open_file_info.hpp"
#include <numeric>

namespace duckdb {

enum class MultiFileFileState : uint8_t { UNOPENED, OPENING, OPEN, SKIPPED, CLOSED };

class DeleteFilter {
public:
	virtual ~DeleteFilter() = default;

public:
	virtual idx_t Filter(row_t start_row_index, idx_t count, SelectionVector &result_sel) = 0;
};

struct HivePartitioningIndex {
	HivePartitioningIndex(string value, idx_t index);

	string value;
	idx_t index;

	DUCKDB_API void Serialize(Serializer &serializer) const;
	DUCKDB_API static HivePartitioningIndex Deserialize(Deserializer &deserializer);
};

struct MultiFileColumnDefinition {
public:
	MultiFileColumnDefinition(const Identifier &name, const LogicalType &type) : name(name), type(type) {
	}
	MultiFileColumnDefinition(const char *name, const LogicalType &type) : name(name), type(type) {
	}
	MultiFileColumnDefinition(const string &name, const LogicalType &type) : name(name), type(type) {
	}

	MultiFileColumnDefinition(const MultiFileColumnDefinition &other)
	    : name(other.name), type(other.type), children(other.children),
	      default_expression(other.default_expression ? other.default_expression->Copy() : nullptr),
	      identifier(other.identifier) {
	}

	MultiFileColumnDefinition &operator=(const MultiFileColumnDefinition &other) {
		if (this != &other) {
			name = other.name;
			type = other.type;
			children = other.children;
			default_expression = other.default_expression ? other.default_expression->Copy() : nullptr;
			identifier = other.identifier;
		}
		return *this;
	}

public:
	static MultiFileColumnDefinition CreateFromNameAndType(const Identifier &name, const LogicalType &type) {
		MultiFileColumnDefinition result(name, type);
		if (type.id() == LogicalTypeId::STRUCT) {
			// recursively create for children
			for (auto &child_entry : StructType::GetChildTypes(type)) {
				result.children.push_back(CreateFromNameAndType(child_entry.first, child_entry.second));
			}
		}
		return result;
	}

	static vector<MultiFileColumnDefinition> ColumnsFromNamesAndTypes(const vector<Identifier> &names,
	                                                                  const vector<LogicalType> &types) {
		vector<MultiFileColumnDefinition> columns;
		D_ASSERT(names.size() == types.size());
		for (idx_t i = 0; i < names.size(); i++) {
			auto &name = names[i];
			auto &type = types[i];
			columns.push_back(CreateFromNameAndType(name, type));
		}
		return columns;
	}

	static void ExtractNamesAndTypes(const vector<MultiFileColumnDefinition> &columns, vector<string> &names,
	                                 vector<LogicalType> &types) {
		D_ASSERT(names.empty());
		D_ASSERT(types.empty());
		for (auto &column : columns) {
			names.push_back(column.name.GetIdentifierName());
			types.push_back(column.type);
		}
	}

	int32_t GetIdentifierFieldId() const {
		D_ASSERT(!identifier.IsNull());
		D_ASSERT(identifier.type().id() == LogicalTypeId::INTEGER);
		return identifier.GetValue<int32_t>();
	}

	string GetIdentifierName() const {
		if (identifier.IsNull()) {
			// No identifier was provided, assume the name as the identifier
			return name.GetIdentifierName();
		}
		D_ASSERT(identifier.type().id() == LogicalTypeId::VARCHAR);
		return identifier.GetValue<string>();
	}

public:
	Identifier name;
	LogicalType type;
	vector<MultiFileColumnDefinition> children;
	unique_ptr<ParsedExpression> default_expression;

	//! Either the field_id or the name to map on
	Value identifier;
};

//! fwd declare
struct MultiFileCastMap;
struct MultiFileFilterEntry;
//! Defined in `src/common/multi_file/multi_file_rust_bridge.hpp`, which is the only place that knows both sides
struct MultiFileFilterEntryAccess;

//! MultiFileLocalColumnId refers to an absolute column index within a specific file we are reading
// i.e. if the file has columns "a, b, c", a will have MultiFileLocalColumnId(0), b = 1, c = 2
struct MultiFileLocalColumnId {
	friend struct MultiFileCastMap;

public:
	explicit MultiFileLocalColumnId(column_t column_id) : column_id(column_id) {
	}

public:
	operator idx_t() const { // NOLINT: allow implicit conversion
		return column_id;
	}
	idx_t GetId() const {
		return column_id;
	}

private:
	column_t column_id;
};

//! fwd declare
template <class T>
struct MultiFileLocalColumnIds;
struct MultiFileGlobalColumnIds;
struct MultiFileColumnMapping;
struct MultiFileFilterMap;
struct MultiFileConstantMap;

//! Resolves a projection position against a list of `size` column ids, refusing one that is invalid or out of range.
//! `MultiFileLocalColumnIds::operator[]` goes through this rather than indexing its vector directly. The rule is the
//! Rust module's, hence the out-of-line definition; it is a free function so that it works for every `T` the
//! `MultiFileLocalColumnIds` template is instantiated with.
DUCKDB_API idx_t ResolveMultiFileLocalIndex(idx_t size, idx_t index);

//! MultiFileLocalIndex refers to a ProjectionIndex locally within a specific file that we are reading
//! This refers to an entry in the "column_ids" list - i.e. to get the absolute column (MultiFileLocalColumnId)
// we can use column_ids[#MultiFileLocalColumnId]
struct MultiFileLocalIndex {
	//! these are allowed to access the index
	template <class T>
	friend struct MultiFileLocalColumnIds;
	friend struct MultiFileColumnMapping;
	friend struct MultiFileFilterEntry;

public:
	explicit MultiFileLocalIndex(idx_t index = DConstants::INVALID_INDEX) : index(index) {
	}
	explicit MultiFileLocalIndex(ProjectionIndex projection) : index(projection) {
	}

public:
	DUCKDB_API idx_t GetIndex() const;
	DUCKDB_API bool IsValid() const;

	operator idx_t() const { // NOLINT: allow implicit conversion
		return GetIndex();
	}
	operator ProjectionIndex() const { // NOLINT: allow implicit conversion to projection index
		return ProjectionIndex(index);
	}

	inline bool operator==(const MultiFileLocalIndex &rhs) const {
		return index == rhs.index;
	};
	inline bool operator<(const MultiFileLocalIndex &rhs) const {
		return index < rhs.index;
	};
	bool operator!=(const MultiFileLocalIndex &other) const {
		return !(*this == other);
	}
	bool operator>(const MultiFileLocalIndex &other) const {
		return other < *this;
	}
	bool operator<=(const MultiFileLocalIndex &other) const {
		return !(other < *this);
	}
	bool operator>=(const MultiFileLocalIndex &other) const {
		return !(*this < other);
	}

private:
	idx_t index;
};

struct MultiFileGlobalIndex {
	friend struct MultiFileGlobalColumnIds;
	friend struct MultiFileFilterMap;

public:
	explicit MultiFileGlobalIndex(idx_t index) : index(index) {
	}

public:
	operator idx_t() { // NOLINT: allow implicit conversion
		return index;
	}
	idx_t GetIndex() const {
		return index;
	}
	operator ProjectionIndex() const { // NOLINT: allow implicit conversion to projection index
		return ProjectionIndex(index);
	}

	inline bool operator==(const MultiFileGlobalIndex &rhs) const {
		return index == rhs.index;
	};
	inline bool operator<(const MultiFileGlobalIndex &rhs) const {
		return index < rhs.index;
	};
	bool operator!=(const MultiFileGlobalIndex &other) const {
		return !(*this == other);
	}
	bool operator>(const MultiFileGlobalIndex &other) const {
		return other < *this;
	}
	bool operator<=(const MultiFileGlobalIndex &other) const {
		return !(other < *this);
	}
	bool operator>=(const MultiFileGlobalIndex &other) const {
		return !(*this < other);
	}

private:
	idx_t index;
};

struct MultiFileConstantEntry {
	MultiFileConstantEntry(MultiFileGlobalIndex column_idx, Value value_p)
	    : column_idx(column_idx), value(std::move(value_p)) {
	}
	//! The (global) column idx to apply the constant value to
	MultiFileGlobalIndex column_idx;
	//! The constant value
	Value value;
};

//! index used to access the constant map
struct MultiFileConstantMapIndex {
	friend struct MultiFileConstantMap;
	friend struct MultiFileFilterEntry;

public:
	explicit MultiFileConstantMapIndex(idx_t index) : index(index) {
	}

public:
private:
	idx_t index;
};

struct MultiFileFilterEntry {
	//! The Rust rules read these three and are the only thing allowed to see them: `MultiFileFilterEntryAccess` is
	//! the bridge's half of that, and `Set` is how an entry is built.
	friend struct MultiFileFilterEntryAccess;

public:
	MultiFileFilterEntry() : is_set(false), is_constant(false), index(DConstants::INVALID_INDEX) {
	}

public:
	DUCKDB_API void Set(MultiFileConstantMapIndex constant_index);
	DUCKDB_API void Set(MultiFileLocalIndex local_index);

	bool IsSet() const {
		return is_set;
	}
	//! Whether the entry is a constant. An entry that is not set is neither, which is an error rather than a `false`.
	DUCKDB_API bool IsConstant() const;
	DUCKDB_API MultiFileConstantMapIndex GetConstantIndex() const;
	DUCKDB_API MultiFileLocalIndex GetLocalIndex() const;

private:
	bool is_set;
	bool is_constant;
	//! ConstantMapIndex if 'is_constant' else this is a LocalIndex
	idx_t index;
};

template <class T>
struct MultiFileLocalColumnIds {
public:
	void push_back(T column_id) { // NOLINT: matching name of std
		column_ids.push_back(column_id);
	}
	template <typename... Args>
	void emplace_back(Args &&...args) {
		column_ids.emplace_back(std::forward<Args>(args)...);
	}
	//! A position that is invalid, or one past the end, is an error rather than a read out of bounds.
	const T &operator[](MultiFileLocalIndex index) const {
		return column_ids[ResolveMultiFileLocalIndex(column_ids.size(), index.index)];
	}
	bool empty() const { // NOLINT: matching name of std
		return column_ids.empty();
	}
	idx_t size() const { // NOLINT: matching name of std
		return column_ids.size();
	}

private:
	vector<T> column_ids;
};

//! Resolves a global index against a `MultiFileFilterMap` of `size` entries, refusing an index the map does not cover.
//! `MultiFileFilterMap::operator[]` goes through this rather than indexing its vector directly, for the same reason
//! `ResolveMultiFileLocalIndex` does above: the rule is the Rust module's.
DUCKDB_API idx_t ResolveMultiFileFilterMapIndex(idx_t size, idx_t index);

//! Resolves a constant-map index against a `MultiFileConstantMap` of `size` entries, refusing an index the map does
//! not cover. `MultiFileConstantMap::operator[]` goes through this for the same reason `ResolveMultiFileFilterMapIndex`
//! does above: the rule is the Rust module's.
DUCKDB_API idx_t ResolveMultiFileConstantMapIndex(idx_t size, idx_t index);

struct MultiFileFilterMap {
public:
	void push_back(const MultiFileFilterEntry &filter_entry) { // NOLINT: matching name of std
		filter_map.push_back(filter_entry);
	}
	//! A global index the map does not cover is an error rather than a read out of bounds.
	DUCKDB_API MultiFileFilterEntry &operator[](MultiFileGlobalIndex global_index);
	void resize(idx_t size) { // NOLINT: matching name of std
		filter_map.resize(size);
	}

private:
	vector<MultiFileFilterEntry> filter_map;
};

struct MultiFileConstantMap {
public:
	using iterator = vector<MultiFileConstantEntry>::iterator;
	using const_iterator = vector<MultiFileConstantEntry>::const_iterator;

public:
	template <typename... Args>
	void Add(Args &&...args) {
		constant_map.emplace_back(std::forward<Args>(args)...);
	}
	//! A constant-map index the map does not cover is an error rather than a read out of bounds.
	const MultiFileConstantEntry &operator[](MultiFileConstantMapIndex constant_index) {
		return constant_map[ResolveMultiFileConstantMapIndex(constant_map.size(), constant_index.index)];
	}
	idx_t size() const { // NOLINT: matching name of std
		return constant_map.size();
	}
	// Iterator support
	iterator begin() {
		return constant_map.begin();
	}
	iterator end() {
		return constant_map.end();
	}
	const_iterator begin() const {
		return constant_map.begin();
	}
	const_iterator end() const {
		return constant_map.end();
	}

private:
	vector<MultiFileConstantEntry> constant_map;
};

} // namespace duckdb

namespace std {

template <>
struct hash<duckdb::MultiFileGlobalIndex> {
	size_t operator()(const duckdb::MultiFileGlobalIndex &global_idx) const {
		return std::hash<uint64_t> {}(global_idx.GetIndex());
	}
};
} // namespace std
