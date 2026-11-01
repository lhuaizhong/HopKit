# HopKit

**面向 Windows 与 macOS 的网络诊断与临时服务工具箱**

[English](README.md) · [macOS 指南](README-macOS.md) · [问题反馈](https://github.com/lhuaizhong/HopKit/issues)

HopKit 集成终端排障、网络测量、文件传输与日志采集，提供桌面 GUI 和独立 CLI。两种入口共享 Rust 核心，为交互操作和自动化脚本提供一致的诊断结果。

**当前版本：`0.1.0-beta.1` — HopKit 首个 Pre-release。** 下方提供 Windows x64、macOS Apple Silicon 和 macOS Intel 原生安装包，包含当前网络排障、多目标持续监测及独立 CLI。更新内容见 [版本记录](CHANGELOG.md)。

## 功能概览

| 工具 | 主要能力 |
| --- | --- |
| 网络排障 | 本机网络概览；分阶段关联 DNS、路由、ICMP、TCP、HTTP/TLS 证据，给出可能原因与下一步建议 |
| 多目标监测 | 网关/外部/业务目标共用时间轴，ICMP/TCP 失败区段、暂停恢复、网络事件、范围前后对比与有界聚合；见 [监测说明](MONITORING.md) |
| 网络概览 / 目标预览 | 查看接口、地址、网关、DNS 和默认路由；规范化目标，不发送探测 |
| Ping | IPv4/IPv6、定次或持续探测、可配置包大小/超时/间隔、延迟与丢包统计 |
| Traceroute | ICMP 逐跳追踪、每跳三次探测、最大跳数、响应地址与目标到达状态 |
| TCPing | 测量 TCP 延迟，区分成功、拒绝与超时 |
| DNS | 系统解析或指定服务器；支持 A/AAAA/CNAME/MX/TXT/NS/PTR/SOA，UDP 截断时回退 TCP |
| NTP | 测量时钟偏差、延迟、层级并识别 KoD；提供临时 NTPv3/v4 服务，不修改系统时间 |
| iperf3 | 内置 TCP/UDP 测速客户端与服务端，支持反向下载、TCP 并行流、UDP 抖动/丢包与实时速率 |
| FTP | 被动模式目录列表及二进制上传、下载；临时 IPv4/IPv6 服务，支持匿名或账号密码登录，默认只读 |
| TFTP | octet 模式上传、下载与临时服务；本地原子写入，服务默认只读 |
| Syslog | UDP 发送与接收、优先级提取、采样预览、保留原始字节的 JSONL 文件及日志轮转 |

## 下载与启动

[HopKit 首个 Pre-release](https://github.com/lhuaizhong/HopKit/releases/tag/v0.1.0-beta.1) · [SHA-256 校验清单](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/SHA256SUMS.txt)

| 平台 | 安装包 |
| --- | --- |
| Windows x64 | [安装版](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-windows-x64-setup.exe) |
| Windows x64 | [含 CLI 的免安装 ZIP](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-windows-x64-portable.zip) · [免安装单 EXE](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-windows-x64-portable.exe) |
| macOS Apple Silicon | [DMG](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-macos-arm64.dmg) · [含 CLI 的应用 ZIP](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-macos-arm64.zip) |
| macOS Intel | [DMG](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-macos-x86_64.dmg) · [含 CLI 的应用 ZIP](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-beta.1/hopkit-0.1.0-beta.1-macos-x86_64.zip) |

Release 另提供 macOS 独立 CLI ZIP、依赖许可、构建信息及校验值。从 netbox 升级请参阅 [迁移说明](MIGRATION.md)。

- **Windows：** 安装程序，或将 ZIP 解压至本地可写目录，运行 `HopKit.exe`。GUI 依赖 [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)，安装版会在缺失时联网下载。安装版与便携 ZIP 包含 CLI，便携单 EXE 仅包含 GUI。
- **macOS：** 按 CPU 架构选择安装包，打开 DMG 并将 `HopKit.app` 拖入 Applications，或解压应用 ZIP。GUI 使用系统 WKWebView。ZIP 提供 CLI，发布 DMG 中的 CLI 位于 `HopKit.app/Contents/Resources/`。详见 [macOS 指南](README-macOS.md)。

Windows 测试版未做代码签名；macOS 使用 ad-hoc 签名，未经 Apple 公证。最低系统版本、旧版 Windows LTSC 及未测试硬件尚未认证，构建成功不能替代实际运行验证。

基本工具可由普通用户运行。安装包不修改 PATH、不安装常驻服务、不添加防火墙规则，也不自动检查更新。CLI 无需 WebView2、Node、Python 或开发工具链。

<a id="终端排障主流程第三阶段源码新增"></a>

## 图形界面使用

1. 在默认的“网络排障”首页查看接口、地址、网关、DNS 和路由。概览每 15 秒刷新本机状态，不向公网发送探测。
2. 输入 IP、域名、`主机:端口`、`[IPv6]:端口` 或 HTTP(S) URL，核对规范化目标；按需展开专业参数，设置源 IP、DNS 对比、超时、路由追踪或附加 CA。
3. 开始排障，查看分阶段证据、可能原因与下一步建议；需要进一步验证时，使用侧栏中的独立工具。
4. 复制结果或导出 JSON。任务结束后，“导出图片 / 复制图片”可将当前统计、图表、表格、语言、风格与筛选结果保存为 PNG。图片仅包含保留的预览，完整接收日志以 Syslog 文件为准。

GUI 首次启动使用英文与“清新”风格。顶部可切换“中文 / English”及“界面风格 → 清新 / 视力友好”，偏好保存在本机。视力友好风格增大字号、加强对比度与焦点提示。CLI 默认英文，`--lang zh-CN` 可切换结果标题；机器字段固定英文，部分测试版帮助与错误信息仍为中英混合。

任务中心最多支持 **4 个并发任务**，保留 **40 项会话任务**；活动任务固定在可筛选历史之前。打开任务不改变当前表单，“重跑”先载入原请求供核对。各工具及操作模式分别保留会话参数。可停止单项或“停止全部”，并保留部分证据。退出 GUI 会停止其任务，独立 CLI 不受影响。任务历史与工具配置不跨重启保存。

Ping/TCPing 按整个任务累计统计，与最近预览分离。中断调用单独计数，不计入延迟与失败率；图表可显示最近 20/80/200 个可用样本，筛选不改变累计指标。Traceroute 按跳报告结果：TTL 到期表示本跳响应，中间跳超时不能单独证明链路断开。

Windows TCPing 优先使用 `SIO_TCP_INFO` 返回的 TCP RTT，排除应用调度和首次调用开销；系统不支持、没有有效 RTT 或发生 SYN 重传时回退到建连耗时。成功样本的 `rtt_source` 标明 `tcp_info` 或 `connect_elapsed`，`connect_elapsed_ms` 始终保留实际建连调用耗时。每次探测使用独立连接，不丢弃首条样本。

## CLI 使用

多目标监测示例：`hk monitor --target ping,192.168.1.1 --target ping,example.com --target tcping,business.example,443 --format jsonl`。在 GUI 侧栏选择 Monitor。采用系统路由，最多三个目标、依次探测，共用现有四任务上限；休眠或采样中断不补造失败。详见 [参数、预算和验证边界](MONITORING.md)。发布验收在三个原生架构上运行协议回归及 35 秒监测故障/恢复检查；两小时以上监测与物理休眠/网卡切换尚待实测。

在解压后的目录中运行：

| 平台 | 完整入口 | 等效短入口 |
| --- | --- | --- |
| Windows PowerShell | `.\hopkit-cli.exe` | `.\shortcuts\hk.exe` |
| macOS 终端 | `./hopkit-cli` | `./shortcuts/hk` |

下文统一使用 `hk`，执行时请替换为上表入口；也可先检查命令冲突，再手动将 `shortcuts` 加入 PATH。短入口是完整 CLI 的同一二进制副本，参数、输出与退出码一致。新包不再分发 `nb`。示例中的局域网地址与文件路径请按实际环境替换。

### 网络诊断

```sh
hk network --format json
hk target 'https://example.com:8443/path' --format json
hk check example.com --format json
hk check 192.168.1.1:443 --timeout-ms 2000 --format json
hk check 'https://example.com/path' --source 192.168.1.10 --format json
hk check example.com --dns-server 192.168.1.1 --trace --format jsonl
hk check 'https://example.com/' --ca-file ./enterprise-ca.pem --format json
hk ping 127.0.0.1 --size 1472
hk ping ::1 -6 -c 4 --format json
hk trace 192.168.1.1 --max-hops 15
hk tcping 192.168.1.1 443 -c 10
hk dns example.com AAAA -s 192.168.1.1
hk ntp 192.168.1.10 -c 4
hk doctor --format json
hk --help
```

Ping 包大小支持 **0～65,500 字节**，默认 **32 字节**，仅指 ICMP 数据，不含 IP/ICMP 头；JSON 使用 `payload_size_bytes` 记录。大包能否到达取决于路径 MTU、分片与设备策略。

### iperf3 网络测速

在一台电脑启动服务端，另一台运行客户端；两端均可使用 HopKit，也可与受支持的标准 iperf3 对端互通。测速由 Rust 内置实现，无需外部 iperf3 程序、libiperf 或运行时下载。

```sh
hk iperf3 serve --listen 0.0.0.0 --port 5201
hk iperf3 client 192.168.1.10 --duration 10 -P 4
hk iperf3 client 192.168.1.10 --duration 10 -R
hk iperf3 client 192.168.1.10 -u --bitrate-mbps 10 --format json
```

TCP 支持 **1～16 个并行流**，目标速率按每流计算，0 表示不限速。UDP 支持 **单流、1～10,000 Mbps**，客户端使用 1,200 字节数据报，并需要同端口 TCP 控制连接。客户端时长为 1～3,600 秒，服务端为 0～86,400 秒，0 表示持续运行。服务端每次处理一个测试，每 0.5 秒更新速率；停止保留部分本端统计，对端指标需完成结果交换后才能取得。

### 文件传输与临时服务

```sh
hk ftp list 192.168.1.10 /backups --user operator
hk ftp get 192.168.1.10 /backups/config.cfg -o ./config.cfg
hk ftp put 192.168.1.10 ./config.cfg --remote-name /backups/config.cfg
hk ftp serve --root ./files --port 2121 --allow 192.168.1.0/24 --write
hk tftp get 192.168.1.10 config.cfg -o ./config.cfg
hk tftp put 192.168.1.10 ./config.cfg
hk tftp serve --root ./files --port 1069 --allow 192.168.1.0/24
hk ntp serve --listen 127.0.0.1 --port 1123 --duration 60 --format jsonl
hk syslog listen --port 1514 --allow 192.168.1.0/24 --dir ./logs
hk syslog send 127.0.0.1 'HopKit test' --port 1514
```

FTP 账号登录前先设置 `HOPKIT_FTP_PASSWORD`，再使用 `--user`：PowerShell 为 `$env:HOPKIT_FTP_PASSWORD='你的密码'`，macOS 为 `export HOPKIT_FTP_PASSWORD='你的密码'`。仅新变量不存在时回退到 `NETBOX_FTP_PASSWORD`；新变量为空仍优先。可用 `--password-env` 指定其他变量。密码不进入缓存、快照或导出，重跑时需重新填写。目标使用主机名或 IP，不含 URL 凭据。

### 输出与退出码

`--format json` 为有限任务输出一个最终对象，也适用于限时服务；`--format jsonl` 逐行输出事件，适用于持续探测与服务。服务默认 `--duration 0`，运行至 Ctrl+C。Trace 使用 `schema_version: 2`，其他命令使用版本 1。

| 退出码 | 含义 |
| --- | --- |
| `0` | 命令完成，并满足该命令适用的诊断判据 |
| `1` | 网络或远端结果未达到判据 |
| `2` | 参数无效 |
| `3` | 本机权限不足、资源冲突或能力不支持 |
| `4` | 本地读写或输出失败 |
| `130` | 主动取消；已有部分结果时予以保留 |

`status=succeeded` 表示流程执行完成，诊断结论需结合 `data.success`（如有）、退出码及各项证据判断。`check` 正常完成仍可记录 HTTP 403/500、证书错误或连接拒绝。Ping/TCPing 至少一次成功即满足默认判据，`--strict` 要求零丢包。

## 使用说明与边界

### 综合排障范围

目标支持 IP、国际化域名、显式端口与 HTTP(S) URL；仅输入主机时不推断 HTTP。URL 中的用户名密码会被拒绝。请求使用原查询参数，但快照、JSON、PNG 与重跑参数移除查询和片段，必要时需重新输入原 URL。“沿用目标”仅向独立工具传递主机及相关端口。

TCP/HTTP 实际绑定所选源 IP，并记录本地与远端地址；系统 DNS、指定 DNS 对比及路由查询遵循系统策略，不绑定源地址。选择源地址时跳过暂不支持绑定的 ICMP/Trace。链路本地 TCP 使用数字 IPv6 接口索引，不支持接口名 scope 或 URL zone identifier。指定源 IP 不保证路由/VPN 变化时仍使用固定物理出口。

HTTP/TLS 使用内置 HTTP/1.1、rustls 与 Mozilla 根证书，不调用 curl/OpenSSL 程序，不使用系统代理/证书库或代理/证书环境变量。**仅发送 HEAD，不读取正文，最多跟随 5 次重定向。** 拒绝带凭据、非 HTTP(S) 或 HTTPS 降级跳转，不发送 Cookie、Authorization、Referer。响应头上限为 64 KiB、128 个字段、8 个临时响应；HEAD 与 GET 的结果可能不同。

默认启用校验；企业 CA 可通过 `--ca-file` 显式添加 PEM 文件，上限 1 MiB。快照只记录路径，不导出文件内容。`--insecure-tls` 会醒目标记已跳过证书链、主机名与时间校验。根证书随版本更新，不支持在线吊销检查或自动补取缺失中间证书。

系统 DNS 最多等待 5 秒；每次 TCP 尝试及每跳完整 HTTP 流程使用所选超时，候选地址最多 8 个，可选 Trace 最长 120 秒。证据时间使用 UTC；阶段耗时包含适配器开销，协议耗时另行记录。部分 ICMP/TCP/DNS 调用可能等到超时才响应取消。单次检查不构成长期可用性或故障归属证明。

### 临时服务配置

默认监听 `0.0.0.0`，即所有 IPv4 网卡；非回环监听的来源清单留空时允许所有来源。`--listen` 可指定本机 IP，`::` 表示所有 IPv6 网卡，`127.0.0.1` / `::1` 表示仅本机。`--allow` 用逗号分隔的 CIDR 限制来源；GUI 在服务端或接收模式中提供相同设置。

| 服务 | 默认端口 | 测试用替代端口 |
| --- | --- | --- |
| FTP | TCP 21 | 2121 |
| NTP | UDP 123 | 1123 |
| TFTP | UDP 69 | 1069 |
| Syslog | UDP 514 | 1514 |
| iperf3 | TCP 5201；UDP 测试另用 UDP 5201 | 任意可用端口 |

端口能否使用取决于权限、占用与防火墙策略；服务不自动提权或切换端口。FTP/TFTP 服务默认只读，`--write` 仅允许新建文件，不覆盖服务端已有文件。请选用可信根目录，避免外部进程同时更改路径。FTP 数据连接与 TFTP 传输使用系统分配的动态端口，防火墙需允许实际流量。

### 协议与存储限制

- **NTP：** 客户端采样 1～20 次。服务使用本机时钟，默认声明层级 10（`--stratum 1..15`），不校验上游同步、不校时；仅支持基本 48 字节、无认证的 NTPv3/v4 请求，不支持 NTS、扩展、广播、对等或控制模式。计数覆盖全部收到的数据报，预览采用采样。
- **iperf3：** 不支持 iperf2、认证、SCTP、同时双向传输、预热、按字节/块数结束、UDP 多流、GSO/GRO，或自定义 MSS/窗口/TOS/拥塞算法。未采集 TCP 重传与 CPU 利用率，协议 CPU 占位值不代表实际测量。
- **FTP：** 明文被动模式，支持 EPSV、IPv4 PASV 回退、最多 4 个控制会话，以及已有子目录的 LIST/NLST/MLSD。无 FTPS/SFTP、主动模式、续传、ASCII 转换、删除、重命名或新建目录。列表上限 10,000 项 / 4 MiB，隐藏符号链接与不安全文件名。本地下载仅在指定 `--overwrite` 且收到成功结束响应后替换文件；其他服务器的上传覆盖策略可能不同，中断后应先核查远端文件。
- **TFTP：** octet 模式、512 字节块、一次一个活动传输、单层文件名，最大文件 **33,553,919 字节**。不支持选项协商、块号回绕、netascii 或续传。每块等待 1 秒、最多重传 5 次，接收端提交后等待 2 秒；最终 ACK 丢失时，即使报告失败，远端文件也可能已经存在。
- **Syslog：** UDP 不保证送达；前 10 条后约按每秒 20 条采样，界面最多显示 200 条事件。完整收到的消息写入 `hopkit-syslog/`，保留 `raw_hex` 与解码文本。既有 `netbox-syslog/` 日志保留原位，不自动合并。
- **日志存储：** 单段 64 MiB，按 7 天 / 约 2 GiB 保留；封段清理，最多允许一段临时超额。每秒及正常关闭时同步写入，剩余空间低于 1 GiB 时停止。强制退出可能留下不完整尾行；不保证生产级吞吐、严格崩溃恢复、SQLite 索引或高级查询。

本测试版不包含 AI/MCP、自动更新与更新签名验证、永久历史、配置导入导出、Syslog TCP/TLS。未启用遥测，诊断探测由用户操作触发。本版不代表原 v0.3 正式方案已通过完整验收。

## 开发与验证

使用 **Node 24**（`.nvmrc`：`24.14.0`）及 **Rust 1.96.0**（`rust-toolchain.toml`），保留依赖锁文件 `package-lock.json` 和 `Cargo.lock`。桌面构建另需 Windows 的 Visual Studio 2022 Build Tools，或 macOS 的 Xcode Command Line Tools。

```sh
git clone https://github.com/lhuaizhong/HopKit.git
cd HopKit
npm ci
npm run build
cargo test -p hopkit-core --locked
cargo build -p hopkit-cli --locked
npm run desktop
```

Windows 发布构建与本地协议复测：

```powershell
cargo build --release --locked -p hopkit-cli
npx tauri build --no-bundle -- --locked
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
```

macOS 打包与分架构检查见 [macOS 指南](README-macOS.md)，安装包组装与验收流程见 [发布工作流](.github/workflows/release.yml)。构建信息、依赖声明、SBOM 与 SHA-256 用于标识产物；每次构建在 ZIP 或安装资源中生成对应产物的元数据，源码目录不保存历史二进制清单。详见 [发布说明](RELEASING.md)。

共用协议集成脚本使用 Python 标准库与独立本地对端，不依赖公网。专项检查包括 `scripts/diagnostic-trust-test.py`、`scripts/iperf3-test.py`、`scripts/check-test.py`，运行前需核对平台与开发依赖。UI 测试使用浏览器 fixture 或测试专用调试，正式启动不开放调试端口。本地证据写入 `test-output/`，包内证据如有提供则位于 `validation/`。结论仅适用于记录的平台与产物，不等同于其他架构、最低系统版本、真实设备或多网卡/VPN 动态环境已通过验证。

## 反馈与许可

请通过 [GitHub Issues](https://github.com/lhuaizhong/HopKit/issues) 提交问题与功能建议，并附应用版本、操作系统/架构、参数、期望与实际结果及导出 JSON。分享前移除敏感地址、配置与日志内容。

版权所有 © 2026 Huaizhong Liu。项目源码采用 [MIT 许可证](LICENSE)；第三方依赖保留各自许可证，详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 及发布包内声明。
