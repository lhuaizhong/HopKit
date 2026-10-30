# netbox macOS 基础功能测试版

版本：`0.1.0-alpha.1`，基于《netbox 技术方案 v0.3》及现有 Windows 基础测试版。GUI 和独立 CLI 共享 Rust 协议核心。此包用于基础评估；不是 v0.3 的正式版完整验收结果。

## 下载

- [Apple Silicon DMG](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-arm64.dmg)
- [Intel DMG](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-x86_64.dmg)
- [全部安装包、CLI ZIP 和 SHA-256](https://github.com/lhuaizhong/netbox/releases/tag/v0.1.0-alpha.1)

DMG 打开后将 `netbox.app` 拖入 Applications；两种 DMG 均以 macOS 11.0 为最低构建目标，最低版本尚未实机验证。

## 启动

- Apple Silicon（M 系列）：使用 `macos-arm64` 包；Intel：使用 `macos-x86_64` 包。Intel 包的构建成功不代表已通过 Intel 实机测试。
- 解压 ZIP，将 `netbox.app` 复制到本地目录，双击打开。无需安装 Node、Rust、Python、WebView2 或 Rosetta；GUI 使用 macOS 自带的 WKWebView。
- CLI 在终端前台独立运行，不启动 GUI。关闭 GUI 会停止该 GUI 的任务；其他终端中运行的 CLI 不受影响。
- 应用使用 ad-hoc 签名，未做 Developer ID 签名和 Apple 公证。对于从网络下载的包，先尝试打开，再按系统“系统设置 → 隐私与安全性 → 仍要打开”的逐应用流程处理。参见 [Apple 官方说明](https://support.apple.com/102445)。不需要全局关闭 Gatekeeper，也不需要清除所有应用的 quarantine 属性。组织策略可能禁止运行未公证应用。

本包不修改 PATH、不安装常驻服务、不调整防火墙、不自动检查更新。首次访问局域网时，如系统要求本地网络权限，仅在需要访问受控设备时允许；拒绝后可在系统设置中调整。本机回环测试不依赖局域网访问权限。

## 基础功能

| 工具 | macOS 测试版功能 |
| --- | --- |
| Ping | Darwin 普通用户 ICMP 数据报套接字；IPv4/IPv6、次数/持续、测试包大小、超时、间隔、延迟、丢包；不调用系统 ping 命令 |
| Traceroute | ICMP 逐跳 TTL / IPv6 hop limit，每跳一行并列显示 3 次探测；区分回复、超时、TTL 到期及不可达 |
| TCPing | TCP connect，区分成功、拒绝和超时 |
| DNS | 系统解析；指定 DNS 的 A/AAAA/CNAME/MX/TXT/NS/PTR/SOA；UDP 截断回退 TCP |
| NTP | 客户端 1～20 次采样、偏差/延迟/stratum、KoD；临时 NTPv3/v4 服务、IPv4/IPv6、来源 CIDR 和统计；不修改系统时间 |
| TFTP | octet / 512 字节，上传、下载、临时服务；原子提交，默认不覆盖，服务默认只读 |
| Syslog | UDP 发送/接收、优先级、采样预览、保留原始字节的 JSONL、按段轮转 |

GUI 提供中文/英文、实时预览、任务中心、取消单任务/全部任务、复制和 JSON 文件导出。最多同时运行 4 个 GUI 任务，保留最近 40 个会话任务。界面展示真实平台及 CPU 架构。文件和日志目录始终由使用者显式选择，不写入 `.app` 内部，不从应用启动位置推断便携数据目录。

NTP 时间服务、TFTP 文件服务、Syslog 接收服务运行或正在停止时，对应“启动服务”按钮禁用；停止完成或启动失败后恢复。切换工具页不解除该状态，可通过“停止”或“停止全部”结束服务。NTP 时间测量、TFTP 下载/上传及 Syslog 发送仍可单独使用。

GUI / CLI 服务默认监听 `0.0.0.0`（所有 IPv4 网卡），对外监听时“允许来源 CIDR”留空允许所有来源，设备使用这台 Mac 的局域网 IP 访问。可指定本机网卡 IP 只在该接口监听，填写 CIDR 限制来源；`::` 表示所有 IPv6 网卡。仅本机测试时可手动填写 `127.0.0.1` / `::1`。

Ping 的“测试包大小 / 字节”可设置为 0～65500 的整数，默认 32。该值仅指 ICMP 数据部分，不包含 IP 头和 8 字节 ICMP 头；结果区与 JSON 的 `payload_size_bytes` 记录本次实际配置。大包能否到达取决于路径 MTU、分片及设备策略，本选项不启用禁止分片或自动探测 MTU。

## CLI 用法

在解压目录打开终端：

```sh
./netbox-cli ping 127.0.0.1
./netbox-cli ping 127.0.0.1 --size 1472
./netbox-cli ping ::1 -6 -c 4 --format json
./netbox-cli trace 127.0.0.1 --max-hops 3
./netbox-cli tcping 192.168.1.1 443 -c 4
./netbox-cli dns localhost
./netbox-cli dns example.com AAAA -s 192.168.1.1
./netbox-cli ntp 192.168.1.10 -c 4
./netbox-cli ntp serve --listen 0.0.0.0 --port 1123 --allow 192.168.1.0/24
./netbox-cli ntp serve --listen ::1 --port 1123 --duration 60 --format jsonl
./netbox-cli tftp get 192.168.1.10 config.cfg -o './配置备份.cfg'
./netbox-cli tftp put 192.168.1.10 './配置备份.cfg'
./netbox-cli tftp serve --root './files' --port 1069
./netbox-cli syslog listen --port 1514 --dir './logs'
./netbox-cli syslog send 127.0.0.1 'netbox 测试' --port 1514
./netbox-cli doctor --format json
./netbox-cli --help
```

示例局域网地址请替换为实际设备地址。NTP 默认 UDP 123，TFTP 默认 UDP 69，Syslog 默认 UDP 514。标准端口是否可用取决于运行环境与端口占用；早期受限环境曾返回 `PERMISSION_DENIED`。遇到权限错误时可手动使用 NTP 1123 / TFTP 1069 / Syslog 1514 测试；GUI 不提权，错误时不会自动换端口。如需只允许某个局域网段，可设置 `--allow 192.168.1.0/24`；多个来源 CIDR 用逗号分隔。

GUI 选择 **NTP → 时间服务** 可设置监听参数、层级（默认 10）和自动停止秒数（0 为手动停止）。服务以本机系统时间响应基本无认证 NTPv3/v4 请求，不校验上游同步、不校时，精度取决于本机时钟。实时显示请求/响应/拒绝/无效报文统计，停止后释放端口。认证/NTS、扩展、广播、对等和控制模式不在支持范围。本次源码能力及完整用法见 [README.zh-CN.md](README.zh-CN.md)，此前发布的二进制包不包含此次新增功能。

可选短入口为 `./shortcuts/nb`，它相对调用包内 CLI。加入 PATH 前先运行 `type -a nb` 检查冲突；本包不覆盖已有命令。

`--format json` 用于有限任务，`--format jsonl` 用于逐行事件或持续任务。服务可用 `--duration <秒>` 自动停止。Ctrl+C 取消。退出码：0 诊断达到判据；1 网络结果未达判据；2 参数无效；3 本机权限/占用/能力不足；4 本地读写/输出失败；130 主动取消。`status=succeeded` 表示执行完成，网络判据请检查 `data.success`。更多共用参数和限制见包内 `README.md`。

## 验证及边界

- 当前完整运行证据来自 Apple Silicon、macOS 27.0.1。本次构建目标下限为 arm64 macOS 11、Intel macOS 10.15；最低系统和 Intel 真机仍需验证，不能从 Mach-O 下限或交叉编译成功推导运行支持。
- TFTP 仅 octet、512 字节、一次一个活动传输、根目录单层文件名；不协商选项、不回绕、不支持 netascii，最大 33,553,919 字节。服务写入只允许新建。最终 ACK 丢失可能导致远端结果不确定，不应盲目重传上传。文件根目录须可信；防火墙需允许实际动态 UDP 传输端口。
- Syslog 仅 UDP，JSONL 段 64 MiB、7 天/约 2 GiB 上限；不承诺正式版吞吐量、SQLite 索引和严格崩溃恢复。预览有界且采样，完整日志以指定目录中 `netbox-syslog/` 文件为准。
- 无 AI/MCP、Syslog TCP/TLS、自动更新或更新签名验证、永久任务历史、FTP。本包的 SHA-256 仅供完整性核对，ad-hoc 签名不证明发布者身份。
- macOS ICMP 等待每 50ms 检查取消；TCP connect 和直接 DNS 的部分系统调用仍可能等到所选超时。实际多网卡/VPN、链路本地 scope、休眠唤醒、物理设备互通及下载 quarantine 首次放行仍应按环境验证。

## 构建和复测

需要 Xcode Command Line Tools、Rust、Node。依赖使用已提交的 Cargo/npm 锁文件。实际构建版本记录在 `BUILD_INFO.json`；本次 macOS 使用 Rust 1.96.0、Node 25.5.0、SDK 27.0，未更新原有协议依赖版本。

```sh
npm ci
cargo test -p netbox-core --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/package-macos.sh --arch arm64
./scripts/package-macos.sh --arch x86_64
python3 scripts/integration_test.py --exe release/netbox-0.1.0-alpha.1-macos-arm64/netbox-cli
python3 scripts/macos_test.py --exe release/netbox-0.1.0-alpha.1-macos-arm64/netbox-cli
```

打包脚本按架构设置 deployment target、生成 `.app` ZIP 和独立 CLI `.tar.gz`，执行 ad-hoc 签名验证，附带构建信息、第三方许可、SBOM 和 SHA-256。默认拒绝覆盖已有发布目录；`--skip-build` 只用于打包已经生成的同架构 release 产物。只交叉编译的 Intel 包不能套用 arm64 运行测试报告作为实机证据。
