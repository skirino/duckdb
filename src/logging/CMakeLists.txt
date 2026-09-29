add_library_unity(
  duckdb_logging
  OBJECT
  log_manager.cpp
  log_storage.cpp
  log_types.cpp
  logger.cpp
  logging.cpp)
add_dependencies(duckdb_logging generate_ahirudb_headers)
target_include_directories(duckdb_logging PUBLIC ${CMAKE_BINARY_DIR}/include)
set_target_properties(duckdb_logging PROPERTIES CXX_STANDARD 20 CXX_STANDARD_REQUIRED ON)
set_target_properties(duckdb_logging PROPERTIES INTERPROCEDURAL_OPTIMIZATION FALSE)

set(ALL_OBJECT_FILES
    ${ALL_OBJECT_FILES} $<TARGET_OBJECTS:duckdb_logging>
    PARENT_SCOPE)
