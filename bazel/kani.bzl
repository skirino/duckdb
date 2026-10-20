load("@rules_rust//rust/private:providers.bzl", "CrateInfo", "DepInfo")

def _rust_kani_test_impl(ctx):
    crate = ctx.attr.target[CrateInfo]
    dep_info = ctx.attr.target[DepInfo]
    srcs = crate.srcs.to_list()
    args = ctx.actions.args()
    for s in srcs:
        args.add(s)
    # 4. crate_universe から引き継いだ依存関係を --extern 引数に変換
    # ターゲットが依存するすべてのコンパイル済みライブラリ（.rlib）の集合
    transitive_libs = dep_info.transitive_crates.to_list()
    for dep in transitive_libs:
        # dep.basename は通常 "lib<crate_name>-<hash>.rlib" になるため、
        # 正確なクレート名とのマッピング（--extern name=path）を構築
        # 注意: より厳密に名前を解決する場合、Starlarkのdepsetに含まれる各Fileのパス情報を解析します
        crate_name = dep.basename.split("-")[0].removeprefix("lib")
        args.add("--extern", "{}={}".format(crate_name, dep.path))
    # 今回ダウンロードした Kani のバイナリ群をインプットとして取得
    kani_tools = ctx.files._kani_tools
    runfiles_inputs = srcs + transitive_libs + kani_tools
    ctx.actions.run(
        outputs = [ctx.outputs.executable],
        inputs = runfiles_inputs,
        executable = ctx.executable._kani_launcher,
        arguments = [args],
        mnemonic = "Kani",
    )
    # ctx.actions.write(
    #     output = ctx.outputs.executable,
    #     content = "\n".join([
    #         "#!/bin/bash",
    #         # macOSのサンドボックス環境下でホストのKaniバイナリを安全に呼び出す
    #         # ホスト環境にインストールされている `kani` へのパスが通っている前提
    #         "export PATH=\"$PWD/../+kani_dependency+kani_binaries/bin:$PATH\"",
    #         "{} {} 2>&1".format(ctx.file._kani_launcher.short_path, " ".join([main_src.short_path] + [df.short_path for df in transitive_libs])),
    #     ]),
    #     is_executable = True,
    # )
    return [
        DefaultInfo(
            executable = ctx.outputs.executable,
            runfiles = ctx.runfiles(files = runfiles_inputs),
        )
    ]

rust_kani_test = rule(
    implementation = _rust_kani_test_impl,
    attrs = {
        "target": attr.label(
            doc = "Kani検証を適用する rust_library や rust_binary",
            providers = [CrateInfo],
            mandatory = True,
        ),
        "_kani_tools": attr.label(
            default = Label("@kani_binaries//:kani_executables"),
        ),
        "_kani_launcher": attr.label(
            default = Label("@crates//:kani-verifier__kani"),
            executable = True,
            cfg = "exec",
        ),
    },
    test = True,
)
