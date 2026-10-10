# The build is Bazel's. This Makefile is the driver the CI workflows and the
# contributor docs call; every target below either runs `bazel` or runs one of
# the repository's own Python tasks. Nothing here configures a compiler.
#
#   make release / relassert / reldebug / debug   build
#   make unittest_release / allunit / smoke       test
#   make format-fix / generate-files              the repository's own tooling
#
# The four build types are `.bazelrc` configs, and Bazel gives each its own
# output directory, so switching between them is free rather than a rebuild.

.PHONY: all opt release relassert reldebug debug threadsan clean \
	unit unittest unittest_release unittest_relassert unittest_reldebug allunit \
	smoke unittestarrow unittest_threadsan unittest_threadsan_extra \
	test_release_tag alltest_release_tag test_ci \
	symbol-checks symbol-leakage-check banned-symbol-check \
	benchmark toolsci format_tools parser_tools spell_tools format_venv capigen_venv \
	enum-integrity-check extension-patch-check check-extension-entries \
	format-check format-check-silent format-fix format-head format-changes \
	format-main format-feature format-configs format-parser-grammar \
	parser-grammar-tools parser-grammar generate-files-deps generate-files \
	relassert-artifact release-artifact cli-release-artifact shared-libs-release-artifact \
	static-libs-release-artifact docs doxygen test_benchmark_sql \
	test_compile test_storage test_vector test_table_scan test_configs \
	test_configs_query_verification test_configs_execution test_configs_persistence \
	test_configs_storage_engine sync_out_of_tree_extensions

all: release
opt: release

BAZEL ?= bazel
PYTHON ?= python3

# A build type is a `.bazelrc` config. The default (no config) is the release
# build, which is also the one `bazel build //...` and `bazel test //...` use.
RELEASE_CONFIG ?=
RELOWITHDEBINFO_CONFIG ?= --config=reldebug
DEBUG_CONFIG ?= --config=debug
THREADSAN_CONFIG ?= --config=threadsan

MKFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
PROJ_DIR := $(dir $(MKFILE_PATH))

CI_CPU_COUNT := $(shell nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || printf '%s\n' "$${NUMBER_OF_PROCESSORS:-1}")
CI_TIDY_JOBS := $(shell jobs=$$(( $(CI_CPU_COUNT) * 50 / 100 )); [ $$jobs -lt 1 ] && jobs=1; echo $$jobs)
export CI_TIDY_JOBS

FORMAT_VENV ?= .cache/format-venv
FORMAT_PYTHON := $(FORMAT_VENV)/bin/python
FORMAT_SETUP_DEPS := format_venv
CAPIGEN_VENV ?= .cache/capigen-venv
CAPIGEN_PYTHON := $(CAPIGEN_VENV)/bin/python
CAPIGEN_SETUP_DEPS := capigen_venv

EXE_SUFFIX :=
ifeq ($(OS),Windows_NT)
EXE_SUFFIX := .exe
endif
UNITTEST_BINARY ?= test/unittest$(EXE_SUFFIX)

# The suite runs through `bazel test`, so a `T=...` argument has to be one the
# unittest binary itself understands -- it goes to the binary as a test
# argument, not to `scripts/ci/run_tests.py`, which is not on this path.
T ?=
BAZEL_TEST_ARGS := $(foreach arg,$(T),--test_arg=$(arg))

# Whether a test target builds the tree it runs against. Most CI jobs that only
# test did not build anything: the job that needed a build published
# `build/<type>/` as an artifact and this one downloaded it, so building again
# throws that artifact away and costs a whole second compile. `SKIP_BUILD=1`
# asks for the same thing locally, on a tree that has already been built.
ifeq ($(SKIP_BUILD)$(CI),)
NEEDS_RELEASE = release
NEEDS_RELASSERT = relassert
NEEDS_REDEBUG = reldebug
NEEDS_DEBUG = debug
NEEDS_THREADSAN = threadsan
else
NEEDS_RELEASE =
NEEDS_RELASSERT =
NEEDS_REDEBUG =
NEEDS_DEBUG =
NEEDS_THREADSAN =
endif

# Non-CI: build prerequisites apply (they did before, via ifndef CI).
# CI: no build prerequisites; the artifact is downloaded.
ifndef CI
CI_NEEDS_RELEASE = release
CI_NEEDS_RELASSERT = relassert
CI_NEEDS_REDEBUG = reldebug
CI_NEEDS_DEBUG = debug
CI_NEEDS_THREADSAN = threadsan
else
CI_NEEDS_RELEASE =
CI_NEEDS_RELASSERT =
CI_NEEDS_REDEBUG =
CI_NEEDS_DEBUG =
CI_NEEDS_THREADSAN =
endif

# The wrapper that `make smoke` and `make unittest_threadsan` run. Both go
# through it rather than through `bazel test`, because the flags they take
# (`--changed-tests`, `--workers`, `--test-config`) belong to
# `scripts/ci/run_tests.py`, which is not on the `bazel test` path. The CI jobs
# that call them build first and point the runner at what they built; relassert
# is the default for a contributor running `make smoke` after `make relassert`,
# threadsan the default for `make unittest_threadsan`.
SMOKE_RUNNER ?= build/relassert/test/run
THREADSAN_RUNNER ?= build/threadsan/test/run

# Bazel owns the binaries; build/<type> is where the rest of the repository --
# the artifact scripts, the workflow `run:` steps, the docs -- looks for them.
# This links those paths to Bazel's output, so nothing has to copy a binary it
# does not own.
define publish_build
	$(PYTHON) scripts/publish_build.py --output-dir build/$(1) --config=$(2) --unittest-name unittest$(EXE_SUFFIX)
endef

# --- build types --------------------------------------------------------------

release:
	$(BAZEL) build $(RELEASE_CONFIG) //...
	$(call publish_build,release,$(RELEASE_CONFIG))

relassert:
	$(BAZEL) build --config=relassert //...
	$(call publish_build,relassert,--config=relassert)

reldebug:
	$(BAZEL) build $(RELOWITHDEBINFO_CONFIG) //...
	$(call publish_build,reldebug,$(RELOWITHDEBINFO_CONFIG))

debug:
	$(BAZEL) build $(DEBUG_CONFIG) //...
	$(call publish_build,debug,$(DEBUG_CONFIG))

# The release configuration built with the thread sanitizer. It is a build type
# of its own because the sanitizer only reaches a binary through its
# compilation flags: building `reldebug` and running it under TSAN_OPTIONS tests
# an uninstrumented build.
threadsan:
	$(BAZEL) build $(THREADSAN_CONFIG) //...
	$(call publish_build,threadsan,$(THREADSAN_CONFIG))

benchmark:
	$(BAZEL) build $(RELEASE_CONFIG) //:benchmark_runner
	$(call publish_build,benchmark,$(RELEASE_CONFIG))

clean:
	$(BAZEL) clean
	rm -rf build

# --- tests --------------------------------------------------------------------

# `bazel test //...` is the suite: the targets split it by directory so they run
# in parallel and a failure names the directory it came from.
unittest: $(NEEDS_DEBUG)
	$(BAZEL) test $(DEBUG_CONFIG) //...

unit: unittest

unittest_release: $(NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //...

unittest_relassert: $(CI_NEEDS_RELASSERT)
	$(BAZEL) test --config=relassert //...

unittest_reldebug: $(NEEDS_REDEBUG)
	$(BAZEL) test $(RELOWITHDEBINFO_CONFIG) //...

allunit: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... --jobs='HOST_CPUS*.5'

# The smoke list through the published wrapper: `--changed-tests` intersects it
# with the files a PR touched, and only run_tests.py knows how to do that.
smoke: $(CI_NEEDS_RELASSERT)
	$(SMOKE_RUNNER) --batch-timeout 120 --test-list test/smoke_tests.list $(T)

unittestarrow: $(NEEDS_DEBUG)
	$(BAZEL) test $(DEBUG_CONFIG) //:arrow_cpp_test //:arrow_cpp_slow_test

# The sanitizer runs go through the wrapper, because the selection is Catch2 tag
# patterns and a single named .test file: `[intraquery]`, `[interquery]` and
# `[detailed_profiler]` are tags, not Bazel target names, and `--test_filter`
# selects test names rather than tags. The wrapper takes a pattern list and
# resolves both forms, as it did before Bazel. `--workers` and `--track-runtime`
# are its flags too; Bazel owns the parallelism of `bazel test`, but not of a
# wrapper run.
#
# There is no `bazel test` here at all: the binary under test is the published
# threadsan build, which is the one compiled with -fsanitize=thread, so
# `$(THREADSAN_RUNNER)` -- and not a per-directory `bazel test` target -- is
# what runs it.
UNITTEST_HUGE_FLAGS ?= --workers=50% --track-runtime=100
THREADSAN_TESTS = "[intraquery],[interquery],[detailed_profiler],test/sql/tpch/tpch_sf01.test_slow"

unittest_threadsan: export TSAN_OPTIONS ?= suppressions=./.sanitizer-thread-suppressions.txt
unittest_threadsan:
	$(THREADSAN_RUNNER) $(UNITTEST_HUGE_FLAGS) --test-config test/configs/threadsan.json $(THREADSAN_TESTS) $(T)
	$(THREADSAN_RUNNER) $(UNITTEST_HUGE_FLAGS) --test-config test/configs/threadsan.json \
		--test-flags="--force-storage --force-reload" "[interquery]" $(T)

unittest_threadsan_extra: export TSAN_OPTIONS ?= suppressions=./.sanitizer-thread-suppressions.txt
unittest_threadsan_extra:
	$(THREADSAN_RUNNER) $(UNITTEST_HUGE_FLAGS) --test-config test/configs/threadsan.json \
		--test-flags="--force-storage" "[interquery]" $(T)

# `alltest_release_tag` adds the `*` Catch2 spec, which is the whole difference
# from `test_release_tag`: it asks for the hidden cases as well as the default
# set. Quoted so the shell hands Bazel the asterisk instead of globbing it.
test_release_tag: $(NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... --test_arg=--select-tag --test_arg=release

alltest_release_tag: $(NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... --test_arg=--select-tag --test_arg=release --test_arg='*'

TEST_CONFIGS_QUERY_VERIFICATION := \
	test/configs/verify_statement_copy.json \
	test/configs/verify_statement_to_string.json \
	test/configs/verify_statement_explain.json \
	test/configs/verify_statement_prepare.json \
	test/configs/verify_serializer.json \
	test/configs/verify_stats.json \
	test/configs/verify_statement_serialization.json \
	test/configs/disable_optimizer.json \
	test/configs/verification_projection.json \
	test/configs/verify_column_bindings.json \
	test/configs/heap_based_parser.json

TEST_CONFIGS_EXECUTION := \
	test/configs/internal_vector_serialization.json \
	test/configs/internal_vector_verification.json \
	test/configs/force_external.json \
	test/configs/verify_fetch_row.json \
	test/configs/disable_caching_operators.json \
	test/configs/variant_vector.json \
	test/configs/verify_aggregate_state_export.json \
	test/configs/verify_functions.json \
	test/configs/shredded_vector.json

TEST_CONFIGS_PERSISTENCE := \
	test/configs/force_storage.json \
	test/configs/force_storage_restart.json \
	test/configs/latest_storage.json \
	test/configs/wal_verification.json \
	test/configs/vacuum_rebuild_indexes_force_storage.json \
	test/configs/no_local_filesystem.json \
	test/configs/encryption.json \
	test/configs/v1_storage.json \
	test/configs/v1_storage_block_size_16kB.json

TEST_CONFIGS_STORAGE_ENGINE := \
	test/configs/verify_compression.json \
	test/configs/block_verification.json \
	test/configs/block_verification_latest.json \
	test/configs/block_size_16kB.json \
	test/configs/latest_storage_block_size_16kB.json \
	test/configs/block_allocator_100mib.json \
	test/configs/compressed_in_memory.json \
	test/configs/prefetch_all_storage.json \
	test/configs/force_storage_mmap.json

TEST_CONFIGS := $(TEST_CONFIGS_QUERY_VERIFICATION) $(TEST_CONFIGS_EXECUTION) $(TEST_CONFIGS_PERSISTENCE) \
	$(TEST_CONFIGS_STORAGE_ENGINE)

# `--test-config` is matched as a bare flag by
# TestConfiguration::ParseArgument, which then consumes the next argument, so
# each configuration is two test arguments rather than one joined with `=`.
# Recursive (`=`, not `:=`) so that `$(1)` is the $(call)'s argument and not
# whatever happened to be set when the Makefile was parsed.
TEST_CONFIG_ARGS = $(foreach cfg,$(1),--test_arg=--test-config --test_arg=$(cfg))

test_configs: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... $(call TEST_CONFIG_ARGS,$(TEST_CONFIGS))

test_configs_query_verification: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... $(call TEST_CONFIG_ARGS,$(TEST_CONFIGS_QUERY_VERIFICATION))

test_configs_execution: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... $(call TEST_CONFIG_ARGS,$(TEST_CONFIGS_EXECUTION))

test_configs_persistence: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... $(call TEST_CONFIG_ARGS,$(TEST_CONFIGS_PERSISTENCE))

test_configs_storage_engine: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... $(call TEST_CONFIG_ARGS,$(TEST_CONFIGS_STORAGE_ENGINE))

# One run per vector type, as before: `--verify-vector` takes a single value and
# repeated runs are the only way to cover each of them.
test_vector: $(CI_NEEDS_RELEASE)
	$(foreach v,dictionary_expression dictionary_operator constant_operator sequence_operator nested_shuffle, \
		$(BAZEL) test $(RELEASE_CONFIG) //... --test_arg=--verify-vector --test_arg=$(v) --test_arg=--skip-compiled; )

test_table_scan: $(CI_NEEDS_RELEASE)
	$(BAZEL) test $(RELEASE_CONFIG) //... \
		--test_arg=--on-init --test_arg="SET debug_physical_table_scan_execution_strategy=SYNCHRONOUS;"

# Regression against released binaries: the released duckdb downloads each version
# and asks the current one to read and write its files.
test_storage: $(CI_NEEDS_RELEASE)
	$(PYTHON) scripts/test_storage_compatibility.py --versions "1.2.1|1.3.2|1.4.3" --new-unittest build/release/$(UNITTEST_BINARY)

test_benchmark_sql: $(CI_NEEDS_RELASSERT)
	$(PYTHON) -u scripts/test_benchmark_sql_runner.py --shell build/relassert/duckdb

test_compile: # test compilation of individual cpp files
	$(PYTHON) scripts/test_compile.py

# The unit tests of the CI scripts themselves.
test_ci:
	$(PYTHON) -m unittest discover --buffer --start-directory scripts/ci $(T)

# --- symbol checks ------------------------------------------------------------

symbol-checks: symbol-leakage-check banned-symbol-check

symbol-leakage-check:
	$(PYTHON) scripts/exported_symbols_check.py build/release/src/libduckdb*.so*

banned-symbol-check:
	$(PYTHON) scripts/banned_symbols_check.py --directory build/release/src

# --- artifacts ----------------------------------------------------------------

relassert-artifact:
	bash scripts/prepare_build_artifact.sh relassert

release-artifact:
	bash scripts/prepare_build_artifact.sh release

cli-release-artifact:
	bash scripts/package_release_artifact.sh cli "$(ARTIFACT_SUFFIX)" "$(CLI_BINARY)"

shared-libs-release-artifact:
	bash scripts/package_release_artifact.sh shared-libs "$(ARTIFACT_SUFFIX)" $(SHARED_LIBRARIES)

static-libs-release-artifact:
	bash scripts/package_release_artifact.sh static-libs "$(ARTIFACT_SUFFIX)" $(STATIC_LIBRARIES)

# --- docs ---------------------------------------------------------------------

docs:
	doxygen Doxyfile

doxygen: docs

# --- formatting and generated files -------------------------------------------

# The host toolchain the CI images are expected to have, plus the compiler
# versions it reports. Bazel fetches its own C++ toolchain; this is the tools
# around it (the linker, the package manager's cache, the spell checker).
define ensure_apt_commands
	missing=0; \
	for cmd in $(1); do \
		command -v $$cmd >/dev/null 2>&1 || missing=1; \
	done; \
	if [ $$missing -eq 1 ]; then \
		sudo apt-get $(APT_TIMEOUT_OPTS) update -y -q && \
		sudo apt-get $(APT_TIMEOUT_OPTS) install -y -q $(2); \
	fi
endef

define ensure_apt_packages
	missing=0; \
	for pkg in $(1); do \
		dpkg-query -W -f='$${Status}' $$pkg 2>/dev/null | grep -q "install ok installed" || missing=1; \
	done; \
	if [ $$missing -eq 1 ]; then \
		sudo apt-get $(APT_TIMEOUT_OPTS) update -y -q && \
		sudo apt-get $(APT_TIMEOUT_OPTS) install -y -q $(1); \
	fi
endef

APT_TIMEOUT_OPTS=-o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o DPkg::Lock::Timeout=30

toolsci:
	$(call ensure_apt_commands,mold ccache pkg-config pigz clang++-20 clangd-20,mold ccache pkg-config pigz clang++-20 clangd-20)
	$(call ensure_apt_packages,python3-requests libcurl4-openssl-dev llvm-20-dev libclang-rt-20-dev)
	ls -lh /usr/bin/gcc* /usr/bin/g++* /usr/bin/clang++*
	gcc --version
	g++ --version
	clang++ --version

format_tools: parser_tools spell_tools

parser_tools:
	sudo pip3 install --break-system-packages cxxheaderparser pcpp
	@echo "::group::Installed Python packages"
	pip3 freeze
	@echo "::endgroup::"

spell_tools:
	@if [ "$$(uname -s)" != "Linux" ]; then \
		echo "Skipping spell_tools on non-Linux"; \
		exit 0; \
	fi
	@set -eu; \
	VERSION=1.45.1; \
	if [ "$$(uname -m)" = "arm64" ] || [ "$$(uname -m)" = "aarch64" ]; then \
		ARCH="aarch64"; \
	else \
		ARCH="x86_64"; \
	fi; \
	TARGET_FILE="$${ARCH}-unknown-linux-musl"; \
	FILE_NAME="typos-v$${VERSION}-$${TARGET_FILE}.tar.gz"; \
	DOWNLOAD_URL="https://github.com/crate-ci/typos/releases/download/v$${VERSION}/$${FILE_NAME}"; \
	$(PYTHON) scripts/ci/retry.py -- curl --fail --location --silent --show-error --output "/tmp/$${FILE_NAME}" "$${DOWNLOAD_URL}"; \
	TMP_DIR="$$(mktemp -d)"; \
	tar -xzf "/tmp/$${FILE_NAME}" -C "$${TMP_DIR}"; \
	TYPOS_BIN="$$(find "$${TMP_DIR}" -type f -name typos | head -n 1)"; \
	if [ -w /usr/local/bin ]; then \
		install -m 0755 "$${TYPOS_BIN}" /usr/local/bin/typos; \
	else \
		sudo install -m 0755 "$${TYPOS_BIN}" /usr/local/bin/typos; \
	fi; \
	typos --version

enum-integrity-check:
	$(PYTHON) scripts/verify_enum_integrity.py src/include/duckdb.h

extension-patch-check:
	$(PYTHON) scripts/check_extension_patches.py --config-dir .github/config --patch-dir .github/patches/extensions

format_venv:
	@if [ ! -x "$(FORMAT_PYTHON)" ]; then \
		mkdir -p "$(dir $(FORMAT_VENV))" && \
		$(PYTHON) -m venv "$(FORMAT_VENV)"; \
	fi
	@$(FORMAT_PYTHON) -m pip show black >/dev/null 2>&1 || $(FORMAT_PYTHON) -m pip install black==24.*
	@$(FORMAT_PYTHON) -m pip show clang_format >/dev/null 2>&1 || $(FORMAT_PYTHON) -m pip install clang_format==11.0.1

# Venv holding capigen (the C API generator) and the formatters it needs, pinned by api_spec/pyproject.toml
capigen_venv:
	@if [ ! -x "$(CAPIGEN_PYTHON)" ]; then \
		mkdir -p "$(dir $(CAPIGEN_VENV))" && \
		$(PYTHON) -m venv "$(CAPIGEN_VENV)" && \
		$(CAPIGEN_PYTHON) -m pip install -U pip; \
	fi
	@$(CAPIGEN_PYTHON) -m pip show capigen >/dev/null 2>&1 || $(CAPIGEN_PYTHON) -m pip install --group api_spec/pyproject.toml:generate

format-check: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py --all --check $(T)

format-check-silent: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py --all --check --silent $(T)

format-fix: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py --all --fix --noconfirm $(T)

format-parser-grammar: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py src/include/duckdb/parser/peg/transformer/peg_transformer.hpp --fix --noconfirm
	$(FORMAT_PYTHON) scripts/format.py src/parser/peg/transformer/transform_generated.cpp --fix --noconfirm
	$(FORMAT_PYTHON) scripts/format.py src/parser/peg/transformer/transform_generated_trampoline.cpp --fix --noconfirm
	$(FORMAT_PYTHON) scripts/format.py src/parser/peg/matcher_factory.cpp --fix --noconfirm
	$(FORMAT_PYTHON) scripts/format.py src/parser/peg/matcher.cpp --fix --noconfirm

parser-grammar-tools: $(FORMAT_SETUP_DEPS)
	@$(FORMAT_PYTHON) -m pip show pyyaml >/dev/null 2>&1 || $(FORMAT_PYTHON) -m pip install PyYAML

parser-grammar: parser-grammar-tools
	PYTHON="$(FORMAT_PYTHON)" ./scripts/parser/build_grammar.sh

check-extension-entries: $(FORMAT_SETUP_DEPS)
	$(PYTHON) scripts/generate_extensions_function.py
	$(FORMAT_PYTHON) scripts/format.py src/include/duckdb/main/extension_entries.hpp --fix --noconfirm
	@git diff -- src/include/duckdb/main/extension_entries.hpp > extension_entries.hpp.diff
	@if [ -s extension_entries.hpp.diff ]; then \
		cat extension_entries.hpp.diff; \
		echo "error: differences found in src/include/duckdb/main/extension_entries.hpp"; \
		exit 1; \
	else \
		rm -f extension_entries.hpp.diff; \
		echo "No differences found"; \
	fi

format-head: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py HEAD --fix --noconfirm $(T)

format-changes: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py HEAD --fix --noconfirm $(T)

format-main: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py main --fix --noconfirm $(T)

format-feature: $(FORMAT_SETUP_DEPS)
	$(FORMAT_PYTHON) scripts/format.py feature --fix --noconfirm $(T)

format-configs:
	$(foreach file, $(wildcard $(CONFIGS_DIR)/*), jq . < "$(file)" > "$(file).tmp" && mv "$(file).tmp" "$(file)" ;)

generate-files-deps:
	$(PYTHON) -m pip install -U pip
	$(PYTHON) -m pip install --group api_spec/pyproject.toml:generate
	$(PYTHON) -m pip install cxxheaderparser pcpp

generate-files: $(CAPIGEN_SETUP_DEPS)
	CAPIGEN_PYTHON="$(abspath $(CAPIGEN_PYTHON))" ./scripts/capi_v1_regen.sh
	$(PYTHON) scripts/generate_functions.py
	$(PYTHON) scripts/generate_metrics.py
	$(PYTHON) scripts/generate_settings.py
	$(PYTHON) scripts/generate_serialization.py
	$(PYTHON) scripts/generate_util.py
	$(PYTHON) scripts/generate_storage_info.py
	$(PYTHON) scripts/generate_enum_util.py
	$(PYTHON) scripts/generate_html_template.py
	$(MAKE) parser-grammar
# Run the formatter again after (re)generating the files
	$(MAKE) format-main

# --- extension sources --------------------------------------------------------

# Clones the pinned out-of-tree extensions into extension/external/<name> and
# applies their patches. Building them needs a Bazel target that does not exist
# yet, so this stops at the checkouts.
sync_out_of_tree_extensions:
	$(PYTHON) scripts/sync_out_of_tree_extensions.py $(if $(BUILD_EXTENSIONS),--build-extensions "$(BUILD_EXTENSIONS)")
