This readme explains what types of extensions there are in DuckDB and how to build them.

# What are DuckDB extensions?
DuckDB extensions are libraries containing additional DuckDB functionality separate from the main codebase. These
extensions can provide added functionality to DuckDB that can/should not live in DuckDB main code for various reasons.
DuckDB extensions can be built in two ways. Firstly, they can be statically linked into DuckDBs executables (duckdb cli,
unittest binary, benchmark runner binary, etc). Doing so will automatically make them available when using these binaries.
Secondly, DuckDB has an extension loading mechanism to dynamically load extension binaries. 

# Extension Types
DuckDB Extensions can de divided into different types: In-tree extensions and out-of-tree extensions. These types refer 
to where the extensions live and who maintains them.

### In-tree extensions
In-tree extensions are extensions that live in the main DuckDB repository. These extensions are considered fundamental 
to DuckDB and/or tie into to DuckDB so deeply that changes to DuckDB are expected to regularly break them.  We aim to 
keep the amount of in-tree extensions to a minimum and strive to move extensions out-of-tree where possible.
### Out-of-tree Extensions (OOTEs)
Out-of-tree extensions live in separate repositories outside the main DuckDB repository. The reasons for moving extensions
out-of-tree can vary. Firstly, moving extensions out of the main DuckDB code-base keeps the core DuckDB code smaller
and less complex. Secondly, keeping extensions out-of-tree can be useful for licensing reasons.

There are two main types of OOTEs. Firstly, there are the **DuckDB Managed OOTEs**. These are distributed through the main
DuckDB CI. These extensions are signed using DuckDBs signing key and are maintained by the DuckDB team. Some examples are 
the `sqlite_scanner` and `postgres_scanner` extensions. The DuckDB Managed OOTEs are distributed automatically with every
release of DuckDB. For the current list of extensions in this category check out `.github/config/out_of_tree_extensions.json`

Secondly, there are **External OOTEs**. Extensions in this category are not tied to the DuckDB CI, but instead their CI/CD
runs in their own repository. The maintainer of the external OOTE repo is responsible for testing, distribution and making
sure that an up-to-date version of the extension is available. Depending on who maintains the extension, these extensions
may or may not be signed.

# Building extensions
Every in-tree extension is one `duckdb_library` in the root `BUILD.bazel`, statically linked
into `//:duckdb_static`, and therefore into the shell, the unittest binary and the benchmark runner.
Adding an extension is adding a target and listing it in the two places that consume the list: the
`extension_codegen_files` call that writes the loader, and the deps of `duckdb_static`.

```python
duckdb_library(
    name = "my_extension",
    srcs = glob(["extension/my_extension/**/*.cpp"]),
    includes = sorted(DUCKDB_INCLUDES + ["extension/my_extension/include"]),
)
```
`DUCKDB_INCLUDES` and `DUCKDB_DEFINES` carry what every target wants; the extension adds only the
include roots its own headers are found through.

Loadable extensions -- the `.duckdb_extension` files a client installs at run time -- are
`cc_shared_library` targets whose output is renamed by a genrule, because the loader finds them by
path. See the `*_loadable_extension` targets near the bottom of `BUILD.bazel` for the shape, and
`scripts/append_extension_metadata.py` for the metadata footer each one carries.

## Extension descriptors
The extensions DuckDB's own CI tracks are listed in `.github/config`, as two kinds of JSON file:

* `.github/config/extensions/<name>.json` pins one out-of-tree extension:
  ```json
  {
    "name": "postgres_scanner",
    "git_url": "https://github.com/duckdb/postgres_scanner",
    "git_tag": "cd043b49cdc9e0d3752535b8333c9433e1007a48",
    "submodules": [],
    "apply_patches": true
  }
  ```
* `.github/config/<group>.json` is a list of extension names that belong together, e.g.
  `.github/config/bundled_extensions.json` is what a release ships.

`scripts/extension_config.py` is the one reader of both. `scripts/sync_out_of_tree_extensions.py`
clones each pinned extension and applies its patches; `scripts/check_extension_patches.py` verifies
that every extension with patches under `.github/patches/extensions/` says `apply_patches`, and that
none says so without any.

# Reporting an extension version

An extension reports its version through `EXT_VERSION_<NAME_UPPERCASE>`, which the build defines to the
version configured at build time:
```cpp
std::string IcuExtension::Version() const {
#ifdef EXT_VERSION_ICU
	return EXT_VERSION_ICU;
#else
	return "";
#endif
}
```
The definition goes on the one library that reads it, not on the shared define list:

```python
duckdb_library(
    name = "icu_extension",
    srcs = glob(["extension/icu/**/*.cpp"]),
    local_defines = DUCKDB_DEFINES + ["EXT_VERSION_ICU"],
)
```

Putting it in `DUCKDB_DEFINES` instead would put a value which changes on every commit into the command line
of every translation unit, forcing a recompile of the whole extension for a string that a single file reads.
Out-of-tree extensions do the same; their version is their own commit hash, so it changes on every commit in
their repository.
