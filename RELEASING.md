# Release builds / 发布构建

Version `0.1.0-beta.1` is HopKit Pre-release 1. Keep the version in `package.json`,
`package-lock.json`, the workspace `Cargo.toml`/`Cargo.lock`, and
`src-tauri/tauri.conf.json` consistent. Do not replace historical release tags.

## Native CI / 原生构建

The [release workflow](.github/workflows/release.yml) runs manually on a selected
commit. It uses Windows x64, macOS Apple Silicon and macOS Intel runners, Node 24,
the pinned Rust toolchain and committed dependency locks. macOS deployment target
is 11.0 for both architectures; this is a build target, not minimum-OS certification.

```sh
gh workflow run release.yml --ref main
gh run list --workflow release.yml
gh run download <run-id> --pattern 'hopkit-*' --dir release/downloads
```

Each runner checks preferences, monitoring range calculations and Rust core tests;
builds the release CLI and GUI; runs offline protocol peers and a 35-second
three-target monitor with injected TCP failure/recovery; generates notices/SBOM;
then bundles and verifies its native packages. Failed validation prevents artifact
publication. Separate `validation-*` artifacts preserve diagnostic reports.

Windows checks silent per-user installation/uninstallation and GUI process startup.
macOS checks both Mach-O architectures, ad-hoc signatures and read-only DMG mounting.
All platforms verify packaged full/short CLI entries, exit codes, Chinese paths and
arguments. ZIPs include `validation/` reports tied to executable SHA-256 values.
GUI startup checks are not a full interactive GUI acceptance test.

工作流分别在三个原生平台构建；协议与监测检查失败时停止打包。安装/归档验收包含
实际分发的 CLI、退出码、中文路径和参数。macOS DMG 校验架构、签名及只读挂载，
Windows 验证安装与卸载。长时间监测及物理休眠/网卡切换需要另行验收。

## Assets / 产物

- `hopkit-<version>-windows-x64-setup.exe`
- `hopkit-<version>-windows-x64-portable.exe` (GUI only / 仅 GUI)
- `hopkit-<version>-windows-x64-portable.zip` (GUI + CLI + `hk` + docs)
- `hopkit-<version>-macos-arm64.dmg` and `macos-x86_64.dmg`
- `hopkit-<version>-macos-<arch>.zip` and `macos-<arch>-cli.zip`
- Per-asset `.sha256` files and a combined `SHA256SUMS.txt`.

Merge all three platform directories into one clean staging directory, retaining
only one combined `SHA256SUMS.txt`; verify every digest before upload. Create the
release against the exact successful build commit, with bilingual notes and the
Pre-release flag. Verify the uploaded asset names, sizes and GitHub SHA-256 digests.

发布时汇总三个平台的校验清单，核对文件 SHA-256，以成功构建的精确提交创建标签，
添加中英发布说明并标记 Pre-release，最后核对 GitHub 上的文件、大小及摘要。

Windows requires WebView2 (installer bootstraps it when absent). Windows builds are
unsigned; macOS uses ad-hoc signing and is not notarized. Artifact-specific
`BUILD_INFO.json`, `SBOM.cdx.json`, `THIRD_PARTY_NOTICES.md` and `licenses/` are
generated in each build and shipped in packages; old binary manifests are not
stored at the source root.
