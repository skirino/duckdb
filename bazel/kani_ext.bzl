load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

def _kani_repo_extension_impl(ctx):
    # ルートモジュールや依存関係から渡されたバージョンタグ情報を走査
    for mod in ctx.modules:
        for tag in mod.tags.install:
            version = tag.version
            
            # macOS のアーキテクチャ判別 (Apple Silicon か Intel か)
            os_arch = ctx.os.arch
            if os_arch == "aarch64" or os_arch == "arm64":
                # Apple Silicon 用プレフィックス (例: kani-vX.Y.Z-aarch64-apple-darwin)
                platform_suffix = "aarch64-apple-darwin"
            else:
                # Intel Mac 用
                platform_suffix = "x86_64-apple-darwin"

            # GitHub の Kani 公式リリースからアーカイブ URL を構築
            # (必要に応じて Kani 側の実際の配布形式に合わせて tar.gz 等を調整してください)
            url = "https://github.com/model-checking/kani/releases/download/kani-{version}/kani-{version}-{platform}.tar.gz".format(
                version = version,
                platform = platform_suffix,
            )

            # 内部で http_archive を発行してリポジトリを作成
            http_archive(
                name = "kani_binaries",
                urls = [url],
                strip_prefix = "kani-" + version,
                # ダウンロードされたディレクトリ内のバイナリを bazel 内の alias や filegroup で公開するための BUILD ファイル
                build_file_content = """
filegroup(
    name = "kani_executables",
    srcs = glob(["**/*"]),
    visibility = ["//visibility:public"],
)
                """,
            )

# タグクラスのスキーマ定義
install_tag = tag_class(
    attrs = {
        "version": attr.string(mandatory = True, doc = "ダウンロードしたいKaniのバージョン (例: '0.45.0')"),
    },
)

# 外部に公開するモジュール拡張
kani_dependency = module_extension(
    implementation = _kani_repo_extension_impl,
    tag_classes = {
        "install": install_tag,
    },
)
