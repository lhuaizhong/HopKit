# Third-party notices / 第三方依赖声明

HopKit source is licensed under MIT. Third-party components retain their own licenses.

Every release build generates an architecture-specific `THIRD_PARTY_NOTICES.md`,
`SBOM.cdx.json`, and `licenses/` directory from the committed Cargo/npm lock files
and the dependencies actually installed on that runner. These are included in the
installer/application resources and portable ZIPs, alongside `BUILD_INFO.json`.

发布包内的同名文件列出对应架构构建的依赖、版本和许可；`licenses/` 保存原始许可文件，
`SBOM.cdx.json` 保存组件清单。请以所下载包内的文件为准。

Generate notices with `python scripts/build-info.py --target <Rust target> --build-dir target/release --output-dir release/metadata`
after building the GUI and CLI. See [RELEASING.md](RELEASING.md).
