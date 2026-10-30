# HopKit 第一阶段命名迁移验证记录

日期：2026-10-30。范围：《HopKit 开发改进指引》第 3 节。基线：`30f77bc`，工作区开发版本 `0.1.0-alpha.1`。当前变更尚未提交；不将基线提交视作包含此次实现。

本报告随验证更新；历史 Windows/macOS 报告与根目录 BUILD_INFO/SBOM 保留原有证据。

## 实施范围

- 产品、窗口标题、CLI 帮助、图片品牌与 PNG/JSON 默认文件名改为 HopKit；Rust crates / 目录与 npm 元数据同步。
- `hopkit-cli` 与 `shortcuts/hk` 采用同一二进制副本；Windows 安装版/ZIP、macOS ZIP/CLI ZIP、发布 DMG 资源均包含两个入口。Windows 单 EXE 仅 GUI。
- 三条打包路径及 CI 更新，发布时重新生成许可证、SBOM、构建元数据、SHA-256，并校验解压/安装后入口。
- 语言/界面风格按新键优先迁移；FTP 新密码变量优先、旧变量回退；密码保持不序列化。
- 新 Syslog 目录、锁与临时传输文件采用新前缀；旧日志保留原位，可用 GUI 入口选取并在文件管理器查看。
- 现有仓库/反馈地址、系统 identifier、MIT 与版权、历史标签/报告不改动。JSON/schema_version/退出码约定不改动，诊断统计与协议逻辑留给下一阶段。

## 验证状态

| 检查 | 结果 | 证据 / 范围 |
| --- | --- | --- |
| Node 24.14.0 生产构建 | 通过 | `npm run build`（类型检查 + Vite） |
| 设置迁移 | 通过 | `npm run test:preferences`，11 项；旧值迁入、新键优先、无效值、存储不可读/不可写、保留旧值 |
| Rust 工作区测试 | 通过 | `cargo test --workspace --locked --offline`，CLI 2 + core 29 = 31 项 |
| 格式与 Clippy | 通过 | `cargo fmt --all --check`；`cargo clippy --workspace --all-targets --features hopkit-desktop/custom-protocol --locked --offline -- -D warnings` |
| 独立协议集成 | 通过 | 开发二进制、release 原始 CLI 和包内签名 CLI 各 26/26；最终证据 `test-output/integration-release-macos-x86_64.json`；含 FTP 新旧密码优先级、空新值、自定义变量与脱敏 |
| CLI ZIP 解压验证 | 通过（开发及 release 二进制） | 两入口 12 类检查；帮助、版本、JSON/JSONL、退出码 0/1/2/3/4/130；中文/空格路径、切换工作目录、引号参数、旧日志保留 |
| 页面显示检查 | 通过（浏览器前端） | browser 技能检查 HopKit、hk、中英文切换后刷新、Syslog 旧日志入口；普通浏览器没有 Tauri IPC，不能代替原生验证 |
| 诊断核心变更边界 | 通过 | diagnostics/dns/lib/macos_icmp/ntp 源文件与基线逐字节相同；其他协议仅改名称/目录前缀 |
| 最终 ZIP 原生启动/退出 | 通过 | 解压到含空格、中文路径，运行 5 秒后正常退出，退出码 0；`test-output/hopkit-migration/native-smoke.json` |
| 原生 release 构建 | 通过 | Tauri release / custom protocol；生成 `target/release/HopKit` 与 `HopKit.app` |
| Tauri 资源中的 CLI 与 hk | 通过 | 真实 bundle 资源归档后解压，12 类入口检查 + SHA-256 |
| 最终完整 ZIP 与 CLI ZIP | 通过 | `scripts/package-macos.py --arch x86_64 --skip-build --build-dir target/release`；ad-hoc 签名与严格验证，267 项依赖元数据，解压后双入口 12 类检查及逐文件 SHA-256 |

## 平台边界

执行机为 macOS Intel x86_64。Windows 安装/覆盖、Apple Silicon 实机、DMG 挂载安装/覆盖、正式签名身份与最低系统版本本次尚未验证；CI 包级校验已接入，须在对应平台产出后验收。不能以本机编译/浏览器模拟代替 Windows 或原生 WebView 验收。

## 残留名称分类

- `netbox` 仓库 URL 与 clone 后目录：远端尚未迁移。
- `local.netbox.network-toolkit.preview`：系统身份兼容。
- `netbox-language` / `netbox-appearance`、`NETBOX_FTP_PASSWORD`：迁移读取与测试用例。
- `netbox-syslog/`：说明旧日志位置，代码不自动改写。
- 旧下载名称、历史报告、根目录 BUILD_INFO/SBOM、指引中的基线路径：历史证据。
- `nb`：只出现在迁移说明与原始指引，不再分发。

## 本次产物

- 完整包：`release/hopkit-0.1.0-alpha.1-macos-x86_64.zip`。
- CLI 专用包：`release/hopkit-0.1.0-alpha.1-macos-x86_64-cli.zip`。
- 每个 ZIP 旁提供 `.sha256`；包内 `SHA256SUMS.txt` 记录文件哈希，`BUILD_INFO.json` 记录签名后的实际二进制。
- GUI 最终包二进制 SHA-256：`b65693fdcd3a45c8139ca08793d5e67a2bff814e3823ff39d5bfebe47657a397`。
- 包外完整 CLI 与 hk（两份相同）SHA-256：`ea757da752082d7c7fcc200ad5bec5bd9fee3d6da745c02d62bb36bccd51ac6b`。
- Tauri 内置资源 CLI/hk SHA-256：`8b2ecca7d5d5af3bd1fba812006b4b032d7b67b99d228332d6ab5bb9d592a220`。包外 CLI 在本地打包时追加 ad-hoc 签名，因此字节哈希不同；各自完整入口与 hk 均完全相同。
- 新产物只在本地生成，尚未上传或发布；版本号维持开发基线，README 中的旧下载已明确标为历史版本。
