add_library_unity(
  duckdb_logging
  OBJECT
  log_manager.cpp
  log_rust_bridge.cpp
  log_storage.cpp
  log_types.cpp
  logger.cpp)
add_dependencies(duckdb_logging generate_ahirudb_headers)
# `SYSTEM`, because the crubit-generated bindings are not annotated for
# nullability and offsetof on their fields is not standard-layout: both are
# Clang diagnostics that fire inside the generated headers alone
target_include_directories(duckdb_logging SYSTEM
                           PRIVATE ${CMAKE_BINARY_DIR}/include)
set_target_properties(duckdb_logging PROPERTIES CXX_STANDARD 20
                                                CXX_STANDARD_REQUIRED ON)
set_target_properties(duckdb_logging PROPERTIES INTERPROCEDURAL_OPTIMIZATION
                                                FALSE)
# Unlike the two above, this one fires at the call site in our own translation
# unit (the generated bindings relocate types that are not trivially copyable),
# so the target itself has to be told about it
if("${CMAKE_CXX_COMPILER_ID}" MATCHES "Clang$")
  target_compile_options(duckdb_logging PRIVATE -Wno-nontrivial-memcall)
endif()

set(ALL_OBJECT_FILES
    ${ALL_OBJECT_FILES} $<TARGET_OBJECTS:duckdb_logging>
    PARENT_SCOPE)
