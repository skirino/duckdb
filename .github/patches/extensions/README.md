# Extension patches
Patches in this directory are used to smoothen the process of introducing changes to DuckDB that break compatibility with an
out-of-tree extension. Extensions installed from git urls can automatically apply patches found in this directory. The
`apply_patches` flag should be used to explicitly enable this feature. For example,
lets say our extension descriptor looks like this:

```json
{
  "name": "spatial",
  "git_url": "https://github.com/duckdb/duckdb_spatial",
  "git_tag": "f577b9441793f9170403e489f5d3587e023a945f",
  "submodules": [],
  "apply_patches": true
}
```
In this example, upon cloning the spatial extension, all patches in the `.github/patches/extensions/spatial/*.patch`
will be automatically applied.

Note that the reason for having the `apply_patches` flag explicitly enabled is to make it easier for developers reading
the extension config to detect a patch is present. For this reason, the patching mechanism will actually fail if
`apply_patches` is `true` with no patches in `.github/patches/extensions/<ext>/*.patch`.

# Workflow
Imagine a change to DuckDB is introduced that breaks compatibility with extension X. The
workflow for this is as follows:

### PR #1: breaking change to DuckDB
1. Commit breaking change to DuckDB
2. Git clone extension.
3. In DuckDB repo, go to .github/config/extensions/your_extension.json
4. If `"apply_patches"` is `false` in `your_extension.json`, set it to `true`.
5. In cloned extension, check out the `git_tag` from the `your_extension.json` file.
6. In cloned extension, apply existing patches found in .github/patches/extensions
    ```bash 
    git apply [path to duckdb]/.github/patches/extensions/[your_extension]/fix.patch
    ```
7. Make necessary changes to extension.
8. Generate diff in cloned extension, and forward it to the original fix.patch location (or create the directory if it does not exist):
    ```bash
    git diff > [path to duckdb]/.github/patches/extensions/[your_extension]/fix.patch

9. Commit patch in `.github/patches/extensions/[your_extension]/*.patch` using a descriptive name

> [!TIP]
> If the changes you made are very minor, you can skip the next two steps. (PR #2, PR #3)
>
> The next two steps are for the following scenarios:
>
> 1. You are the extension developer, and either the changes are big, or the patches have accumulated and you want to bump the extension.
> 2. You are not the extension developer, but the changes to the extension are big, so you should follow the next two steps so that it is not so cumbersome for the extension developer to bump the extension.


### PR #2: patch to extension X
- Apply (all) the patch(es) in `.github/patches/extensions/x/*.patch` to extension X.

### PR #3: update extension X in DuckDB
- Remove patches in `.github/patches/extensions/x/*.patch`
- Set `"apply_patches": false` in the descriptor
- Update `git_tag` of the extension in the descriptor