"""configure_file() equivalents for the CMake-generated extension loader.

CMake produces two files from the templates in extension/ (see
extension/CMakeLists.txt): an include header and a loader translation unit, both
driven by the list of statically linked extensions. The Bazel port keeps them as
actions rather than checking in the output.
"""

def _string_list(value):
    if type(value) == "list":
        return value
    return value.to_list()

def _camel_case(name):
    return "".join([part.capitalize() for part in name.split("_")])

def _extension_codegen_impl(ctx):
    header = ctx.actions.declare_file(ctx.attr.header_out)
    loader = ctx.actions.declare_file(ctx.attr.loader_out + ".raw")
    loader_stamped = ctx.actions.declare_file(ctx.attr.loader_out)
    extensions = _string_list(ctx.attr.extensions)

    # Mirrors the two-pass configure_file() in extension/CMakeLists.txt: emit the
    # template, then append one #include per statically linked extension.
    # configure_file() guarantees a trailing newline and the checked-in template
    # has none, so the newline is added here -- without it the first #include is
    # swallowed by the template's trailing // comment.
    ctx.actions.run_shell(
        inputs = [ctx.file.header_template],
        outputs = [header],
        command = " ".join([
            "{ cat \"$1\"; printf '\\n'; } > \"$2\";",
            "for ext in $3; do",
            "printf '#include \"%s_extension.hpp\"\\n' \"$ext\" >> \"$2\";",
            "done",
        ]),
        arguments = [ctx.file.header_template.path, header.path, " ".join(extensions)],
        progress_message = "Expanding %s" % ctx.label,
    )

    register_body = ""
    for name in extensions:
        register_body += "    config.linked_extensions.push_back({\"%s\", [](DuckDB &db) {\n" % name
        register_body += "        db.LoadStaticExtension<%sExtension>();\n" % _camel_case(name)
        register_body += "    }});\n"
    name_list = "\n\t" + ",\n\t".join(["\"%s\"" % n for n in extensions])

    # expand_template() substitutes the substitution KEY as a literal string
    # wherever it appears in the template, not the ${KEY} form CMake uses, so the
    # keys below carry the braces.
    ctx.actions.expand_template(
        template = ctx.file.loader_template,
        output = loader,
        substitutions = {
            "${EXT_CAPI_DECLARATIONS}": "",
            "${EXT_REGISTER_BODY}": register_body,
            "${EXT_NAME_VECTOR_INITIALIZER}": name_list,
            "${EXT_TEST_PATH_INITIALIZER}": "",
        },
    )

    # configure_file() guarantees a trailing newline; expand_template() does not.
    ctx.actions.run_shell(
        inputs = [loader],
        outputs = [loader_stamped],
        command = "{ cat \"$1\"; printf '\\n'; } > \"$2\"",
        arguments = [loader.path, loader_stamped.path],
        progress_message = "Newline-terminating %s" % ctx.label,
    )

    # Both outputs are addressed from the same package, where an implicit label
    # of the form ":generated_extension_headers.hpp" would resolve to a (missing)
    # source file rather than to these outputs, so each is published under its
    # own output group for a filegroup to select.
    return [
        DefaultInfo(files = depset([header, loader_stamped])),
        OutputGroupInfo(
            generated_extension_headers = depset([header]),
            generated_extension_loader = depset([loader_stamped]),
        ),
    ]

extension_codegen_files = rule(
    implementation = _extension_codegen_impl,
    attrs = {
        "extensions": attr.string_list(mandatory = True),
        "header_out": attr.string(mandatory = True),
        "loader_out": attr.string(mandatory = True),
        "header_template": attr.label(
            default = Label("//:extension/generated_extension_headers.hpp.in"),
            allow_single_file = True,
        ),
        "loader_template": attr.label(
            default = Label("//:extension/generated_extension_loader.cpp.in"),
            allow_single_file = True,
        ),
    },
)

def _fixture_tree_impl(ctx):
    """Copy the fixture trees into declared directory outputs.

    Bazel ships a target's data as a runfiles tree of symlinks, and DuckDB's `**`
    crawl skips symlinks (LocalFileSystem::CrawlDirectoryLevel), so a test that
    globs `dir/**` finds nothing under such a tree. The affected shards get real
    copies at the same paths instead, which is what CMake gives them. Symlinks
    inside the fixtures stay symlinks: one test asserts they are not crawled.
    """
    outputs = [ctx.actions.declare_directory(tree) for tree in ctx.attr.trees]
    dest = outputs[0].dirname
    commands = []
    for tree, output in zip(ctx.attr.trees, outputs):
        # `tree/.` copies the contents, since Bazel has already created the
        # declared output directory.
        commands.append("mkdir -p {dest}/{tree} && cp -R {tree}/. {dest}/{tree}/".format(
            tree = tree,
            dest = dest,
        ))
        # Bazel rejects a tree artifact that holds a dangling symlink, and one
        # fixture ships one on purpose. Materialize its target: what that test
        # needs is a symlink the crawl must skip, not an unresolvable one.
        materialize = "cd {dest}/{tree} && find . -type l | " + \
            "while IFS= read -r link; do " + \
            'if [ ! -e "$link" ]; then ' + \
            'mkdir -p "$(dirname "$link")/$(readlink "$link")"; fi; ' + \
            "done"
        commands.append(materialize.format(tree = tree, dest = dest))

    # Un sandboxed: a sandbox materializes every input as a symlink, and cp -R
    # would copy those instead of the file contents, which is the whole point
    # here. Unsandboxed, the sources are the checked out files.
    ctx.actions.run_shell(
        inputs = ctx.files.srcs,
        outputs = outputs,
        command = " && ".join(commands),
        execution_requirements = {"no-sandbox": "1"},
        mnemonic = "FixtureTree",
        progress_message = "Copying the fixture tree of %s" % ctx.label,
        env = {"LC_ALL": "C"},
    )

    return [DefaultInfo(files = depset(outputs))]

fixture_tree = rule(
    implementation = _fixture_tree_impl,
    attrs = {
        "trees": attr.string_list(mandatory = True),
        "srcs": attr.label_list(allow_files = True, mandatory = True),
    },
)
