# HopKit migration / 命名迁移

HopKit is the new name for netbox. 当前开发版本仍为 `0.1.0-alpha.1`；历史下载不等于此分支的新包。仓库继续使用 <https://github.com/lhuaizhong/netbox>，本地 `netbox/` 工作目录无需改名。MIT 许可证与版权归属不变。

| 对象 / Item | 新名称 / Name |
| --- | --- |
| 产品 / GUI | HopKit；`HopKit.exe` / `HopKit.app` |
| 完整 CLI / Full CLI | `hopkit-cli`（Windows: `hopkit-cli.exe`） |
| 短入口 / Shortcut | `shortcuts/hk`（Windows: `shortcuts/hk.exe`） |
| Rust / npm | `hopkit-core`、`hopkit-cli`、`hopkit-desktop` / `hopkit-network-toolkit` |
| 发布资产 / Assets | `hopkit-<version>-<platform>-<arch>` |

两个 CLI 入口是同一二进制的副本，均独立运行，不启动 GUI。新包不分发 `nb`，请将旧脚本改成 `hk` 或完整 CLI 路径。安装程序不修改 PATH，不覆盖已有系统 `hk`；自行加入 PATH 前用 `Get-Command hk -All` / `type -a hk` 检查冲突。无需 PATH 时直接运行完整路径或 `./shortcuts/hk`。

Both CLI entries use identical bytes and never launch the GUI. Replace `nb` in scripts. No package modifies PATH or overwrites an existing system `hk`; use the full CLI path if there is a conflict.

| 发布渠道 / Channel | GUI | 完整 CLI 与 hk / Both CLI entries |
| --- | --- | --- |
| Windows NSIS 安装版 | 安装目录 `HopKit.exe` | 安装目录 `hopkit-cli.exe`、`shortcuts/hk.exe` |
| Windows portable ZIP | `HopKit.exe` | 包根目录 `hopkit-cli.exe`、`shortcuts/hk.exe` |
| Windows portable 单 EXE | GUI only | 不包含；下载 ZIP / Use the ZIP |
| macOS 完整 ZIP | `HopKit.app` | 包根目录 `hopkit-cli`、`shortcuts/hk` |
| macOS CLI 专用 ZIP | 不含 GUI / CLI only | 包根目录 `hopkit-cli`、`shortcuts/hk` |
| macOS 发布 DMG | `HopKit.app` | `HopKit.app/Contents/Resources/hopkit-cli`、`shortcuts/hk` |

`scripts/package-macos.py` 的本地 app ZIP 将 CLI 放在应用旁；CI 发布 DMG 通过 Tauri resources 内置两个入口。两种路径均验证实际归档解压后的入口。示例：

```sh
./hopkit-cli --version
./shortcuts/hk ping 127.0.0.1
"/Applications/HopKit.app/Contents/Resources/hopkit-cli" doctor --format json
```

兼容规则 / Compatibility:

- `hopkit-language`、`hopkit-appearance` 不存在时读取有效的 `netbox-language`、`netbox-appearance`，并写入新键；旧值保留。新键存在时始终优先，即使值无效也不读取旧键。存储不可用时仍能切换。New keys win; valid legacy values migrate only if the new key is absent, with old values preserved.
- 默认 FTP 密码：`HOPKIT_FTP_PASSWORD` 优先，仅不存在时回退 `NETBOX_FTP_PASSWORD`；空新值也优先。`--password-env NAME` 读取指定变量，非默认变量不存在时不回退。客户端和服务器一致；密码不写入报告、日志、配置。The new variable wins, including an empty value; only absence allows legacy fallback. An explicit custom variable never falls back.
- GUI 测试浏览器变量改为 `HOPKIT_BROWSER`，不兼容旧测试变量；仅影响开发脚本。The developer-only browser override is renamed without a legacy alias.
- 新 Syslog 任务只写入所选目录下的 `hopkit-syslog/`，锁文件为 `.hopkit.lock`。旧 `netbox-syslog/` 与 `.netbox.lock` 保持原位，不自动改名或合并。GUI “浏览已有日志…”让用户选择旧目录并在系统文件管理器查看。Existing logs remain untouched; select them with “Browse existing logs…”.
- 临时传输文件改为 `.hopkit-*.part`，已有临时文件不自动迁移；无持久缓存需要搬迁。Temporary transfer files use the new prefix; no persistent application cache needs migration.
- Tauri identifier 暂时保持 `local.netbox.network-toolkit.preview`，以保留系统身份和同一来源的设置；本次不改变数据格式、`schema_version`、JSON 字段或退出码。稳定 identifier 和安装覆盖、签名身份另行验证。The application identifier and result contract stay unchanged.
- 历史标签、测试报告、根目录 `BUILD_INFO.json` / `SBOM.cdx.json` 是旧版证据，不改写为新版本。新打包始终重新生成构建信息、SBOM、许可证及校验和，不复用旧哈希。Historical evidence remains intact; package metadata is freshly generated.

开发使用 Node 24（`.nvmrc`：24.14.0）。用 `nvm use` 后执行 `npm ci`、`npm run build`。Tauri 构建生成 `HopKit` GUI 文件；直接 Cargo 构建仍生成 `hopkit-desktop`，打包请使用 Tauri 构建路径。

发布构建入口：CI `.github/workflows/release.yml`；Windows 本地 `scripts/package.ps1`；macOS 本地 `scripts/package-macos.sh`。CI 在 Tauri bundle 前运行 `scripts/prepare-cli.py` 准备资源副本。`scripts/verify-package.py --archive <ZIP>` 从发布包解压后检查两种入口的帮助、版本、JSON、退出码和参数，包括中文、空格、引号及工作目录变化。
