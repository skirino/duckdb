"""A rule that runs Crubit's bindings generator over a Cargo crate.

The generator is `cargo cpp_api_from_rust`; this wraps it so a Cargo crate gets
the same two artifacts Crubit's own build integration produces upstream:

    crubit_include/crubit/<crate>.h  the generated C++ API, plus the bindings for
                                     whichever Rust std crates it exposes
    lib<crate>_cc_api.a              a staticlib holding the generated C++ shims
                                     together with the crate's own code, which is
                                     what C++ links against

Both land at names Bazel can declare. The generator's own paths carry a cargo
metadata hash (`ahirudb-686892ec84dec4a8/`) and sit under a cargo target
directory holding the whole build graph, so none of that is a declared output:
the generator runs in a scratch directory and only the files that matter are
copied out -- the five generated headers, Crubit's own headers, and the archive.

The action is not sandboxed, and the staticlib it produces is not shareable
between machines. Both follow from the generator: it resolves its
`crubit_support` dependency by absolute path (CRUBIT_SUPPORT_RUST_PATH) and
hands that path to a nested cargo build that reads the crate from outside the
action's inputs, and the manifest cargo generates for the C++ shim names the
crate by absolute path, which ends up in the archive.
"""

# The std crates Crubit emits bindings for alongside the crate's own header.
STDLIB_HEADERS = ["alloc", "core", "proc_macro", "std"]

# Crubit's generator names its output by Rust target triple, and the triple has
# to be the one the C++ toolchain is for. rules_rust keeps its own triple on the
# Rust toolchain, but a BUILD file cannot read it and a select() cannot either, so
# the mapping is stated here. A target built for a platform //rust/BUILD.bazel
# does not name gets an empty triple and fails on it, rather than quietly
# generating bindings for the wrong architecture.
CRUBIT_TARGET_TRIPLE = select({
    "//rust:darwin_arm64": "aarch64-apple-darwin",
    "//rust:darwin_x86_64": "x86_64-apple-darwin",
    "//rust:linux_arm64": "aarch64-unknown-linux-gnu",
    "//rust:linux_x86_64": "x86_64-unknown-linux-gnu",
})

_GENERATE = """
set -eu
generator="$1"
cargo="$2"
rustc="$3"
manifest="$4"
triple="$5"
crate="$6"
include_root="$7"  # the include root, relative to the execroot
archive="$8"
stdlib_headers="$9"
support_headers="${10}"
support_root="${11}"

exec_root="$(pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# `crubit_support` is named by absolute path in the manifest of the crate the
# generator generates, so the path has to be absolute. It is read out of the
# generator's own checkout rather than declared as an input, because that is a
# directory of Crubit's, full of BUILD files that Bazel cannot glob or label
# from here -- see bazel/crubit_toolchain.bzl. A Crubit bump rebuilds the
# generator, which is an input, so this path cannot go stale unnoticed.
export CRUBIT_SUPPORT_RUST_PATH="$exec_root/$(dirname "$generator")/crubit/support/crubit_support"
# cargo runs a subcommand by exec'ing `cargo-<name>` off PATH, and runs rustc off
# it too, so both the generator and the pinned toolchain go on PATH first.
# Absolute, not execroot-relative: cargo spawns rustc with its working directory
# set to the crate, so a relative PATH entry would not resolve from there.
PATH="$exec_root/$(dirname "$generator"):$exec_root/$(dirname "$rustc"):$PATH"
export PATH
# The generator links against its Rust nightly's own shared libraries by @rpath
# and records none, so the loader is told where //bazel:crubit_toolchain put them.
export DYLD_LIBRARY_PATH="$exec_root/$(dirname "$generator")${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="$DYLD_LIBRARY_PATH"

"$cargo" cpp_api_from_rust \\
  --manifest-path "$exec_root/$manifest" \\
  --target-dir "$scratch" \\
  -- \\
  --lib \\
  --target="$triple" \\
  --package "$crate" \\
  --release \\
  -- -Cdefault-linker-libraries=yes

for name in "$crate" $stdlib_headers; do
  cp "$scratch/$triple/release/include/crubit/$name.h" \\
     "$exec_root/$include_root/crubit/$name.h"
done

for rel in $support_headers; do
  mkdir -p "$exec_root/$include_root/$(dirname "$rel")"
  cp "$exec_root/$support_root/$rel" "$exec_root/$include_root/$rel"
done

# The staticlib's own path carries the metadata hash, so it is globbed rather
# than named.
cp "$(ls "$scratch/$triple/release/deps"/*/target/release/lib"$crate"_cc_api.a)" \\
   "$exec_root/$archive"
"""

# The one directory //bazel:crubit_toolchain.bzl copies Crubit's headers into.
# Their paths have to be taken relative to this and not to the deepest directory
# holding all of them, which is `crubit_headers/support/` -- and losing that
# `support/` is exactly what the generated bindings include by.
_SUPPORT_HEADERS_ROOT = "crubit_headers"

def _paths_under_headers_root(paths):
    """Splits paths into the headers root they share and their paths under it."""
    marker = "/" + _SUPPORT_HEADERS_ROOT + "/"
    roots = set()
    tails = []
    for path in paths:
        head, found, tail = path.partition(marker)
        if not found:
            fail("%s is not under %s; //bazel:crubit_toolchain.bzl and this rule " +
                 "have to agree on where Crubit's headers are copied to" %
                 (path, _SUPPORT_HEADERS_ROOT))
        roots.add(head + "/" + _SUPPORT_HEADERS_ROOT)
        tails.append(tail)
    if len(roots) != 1:
        fail("Crubit's headers are spread over %s, expected one directory" % roots)
    return roots.pop(), tails

def _crubit_bindings_impl(ctx):
    triple = ctx.attr.target_triple
    crate = ctx.attr.crate_name
    names = [crate] + STDLIB_HEADERS

    headers = [
        ctx.actions.declare_file(
            "{inc}/crubit/{name}.h".format(inc = ctx.attr.include_dir, name = name),
        )
        for name in names
    ]
    archive = ctx.actions.declare_file(ctx.attr.archive)

    # Crubit's own headers come along into the same include root, because the
    # generated bindings include them with angle brackets
    # (`#include <support/bridge.h>`) and a cc_library's `includes` reaches only
    # its direct dependents, so a consumer would otherwise have to name
    # //bazel:crubit_toolchain's headers itself.
    support = ctx.files.support_headers
    support_root, support_tails = _paths_under_headers_root([f.path for f in support])
    support_outputs = [
        ctx.actions.declare_file(
            "{inc}/{rel}".format(inc = ctx.attr.include_dir, rel = rel),
        )
        for rel in support_tails
    ]

    ctx.actions.run_shell(
        command = _GENERATE,
        arguments = [
            ctx.file.generator.path,
            ctx.file.cargo.path,
            ctx.file.rustc.path,
            ctx.file.manifest.path,
            triple,
            crate,
            archive.dirname + "/" + ctx.attr.include_dir,
            archive.path,
            " ".join(STDLIB_HEADERS),
            " ".join(support_tails),
            support_root,
        ],
        inputs = depset(
            direct = [ctx.file.manifest] + ctx.files.srcs + support +
                     [ctx.file.generator, ctx.file.cargo, ctx.file.rustc],
        ),
        outputs = headers + support_outputs + [archive],
        mnemonic = "CrubitBindings",
        progress_message = "Generating C++ bindings for %s" % crate,
        env = {"LC_ALL": "C"},
        execution_requirements = {"no-sandbox": "1"},
    )

    return [
        DefaultInfo(files = depset(headers + support_outputs)),
        OutputGroupInfo(
            archive = depset([archive]),
            headers = depset(headers),
        ),
    ]

crubit_bindings = rule(
    implementation = _crubit_bindings_impl,
    attrs = {
        "manifest": attr.label(
            mandatory = True,
            allow_single_file = True,
            doc = "The crate's Cargo.toml.",
        ),
        "srcs": attr.label_list(
            mandatory = True,
            allow_files = True,
            doc = "The crate's sources, which the generator compiles.",
        ),
        "crate_name": attr.string(
            mandatory = True,
            doc = "The crate's cargo package name, which is also the C++ namespace.",
        ),
        "target_triple": attr.string(
            mandatory = True,
            doc = "Rust target triple, matching the C++ toolchain.",
        ),
        "include_dir": attr.string(
            default = "crubit_include",
            doc = "Directory the headers are written under, relative to the package.",
        ),
        "archive": attr.string(
            default = "libcrubit_cc_api.a",
            doc = "File name for the staticlib C++ links against.",
        ),
        "support_headers": attr.label(
            default = Label("@crubit_toolchain//:support_headers_files"),
            allow_files = True,
            doc = "Crubit's own headers, which the generated bindings include.",
        ),
        "generator": attr.label(
            default = Label("@crubit_toolchain//:cpp_api_from_rust"),
            allow_single_file = True,
            doc = "The `cargo-cpp_api_from_rust` binary.",
        ),
        "cargo": attr.label(
            default = Label("@crubit_toolchain//:cargo"),
            allow_single_file = True,
            doc = "The generator's own cargo. See bazel/crubit_toolchain.bzl for why " +
                  "this is not rules_rust's.",
        ),
        "rustc": attr.label(
            default = Label("@crubit_toolchain//:rustc"),
            allow_single_file = True,
            doc = "The generator's own rustc, which cargo runs to build the crate.",
        ),
    },
    doc = "Generates the C++ bindings and staticlib for a Cargo crate.",
)



# Every translation unit that includes Crubit's generated bindings needs these.
# The bindings are not annotated for nullability and are not standard-layout, so
# both diagnostics fire inside the generated headers alone -- hundreds of them.
# `includes` passes a plain -I rather than the -isystem that would silence them,
# so the copts below do. The generated headers also need C++20, where the rest of
# this build is C++17, so -std=c++20 belongs here too.
def crubit_copts():
    return [
        "-std=c++20",
        "-Wno-invalid-offsetof",
        "-Wno-nullability-completeness",
    ]
