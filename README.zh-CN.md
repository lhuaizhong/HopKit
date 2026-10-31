# HopKit Windows / macOS 基础功能测试版

[English](README.md)

版本：`0.1.0-alpha.1`。当前源码提供用于操作测试和评估的网络诊断工具、临时 NTP/FTP/TFTP/Syslog 服务，包含桌面 GUI 和独立 CLI。macOS 的启动、构建、架构和验证边界见 [README-macOS.md](README-macOS.md)；下列 Windows 操作说明继续保留。

## 下载

**当前分支已改名 HopKit（原 netbox），尚未发布 HopKit 下载包。下列链接是历史 netbox 版本，不含 `hk`；新名称请从当前源码构建。**

[历史 Release v0.1.0-alpha.1](https://github.com/lhuaizhong/HopKit/releases/tag/v0.1.0-alpha.1) · [SHA256SUMS.txt](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/SHA256SUMS.txt)

| 平台 / 格式 | 下载 |
| --- | --- |
| Windows x64 | [netbox-0.1.0-alpha.1-windows-x64-setup.exe](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-setup.exe) |
| Windows x64 portable EXE | [netbox-0.1.0-alpha.1-windows-x64-portable.exe](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.exe) |
| Windows x64 portable ZIP + CLI | [netbox-0.1.0-alpha.1-windows-x64-portable.zip](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.zip) |
| macOS Apple Silicon DMG | [netbox-0.1.0-alpha.1-macos-arm64.dmg](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-arm64.dmg) |
| macOS Intel DMG | [netbox-0.1.0-alpha.1-macos-x86_64.dmg](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-x86_64.dmg) |

macOS 含 CLI 的 ZIP 及每个文件的 SHA-256 也在 Release 页面提供。Windows 安装版包含 GUI 和 CLI，缺少 WebView2 时会联网安装运行库；portable EXE 可直接运行，CLI 见 ZIP 包。macOS 打开 DMG 后将 HopKit 拖入 Applications。当前为预发布测试版：Windows 未做代码签名，macOS 使用 ad-hoc 签名且未经 Apple 公证；系统可能提示发布者无法验证。macOS 两种架构均以 11.0 为最低构建目标，最低系统版本尚未实机验证。

## 启动

解压发布包到本地可写目录，双击 `HopKit.exe`。GUI 使用系统 Microsoft Edge WebView2 Runtime；通常已随较新 Windows 安装。如果 GUI 无法启动，可先独立使用 `hopkit-cli.exe`，再按微软官方说明安装 WebView2 Runtime：<https://developer.microsoft.com/microsoft-edge/webview2/>。

本包不修改 PATH、不安装系统服务、不添加防火墙规则、不自动访问更新服务器。普通用户可执行基本工具；设备访问临时服务时可能需要由用户按本机策略放行入站流量。

## 已实现

| 工具 | GUI / CLI 能力 |
| --- | --- |
| Ping | Windows ICMP API / macOS Darwin ICMP，IPv4/IPv6、次数/持续探测、测试包大小、超时、间隔、延迟和丢包 |
| Traceroute | ICMP 逐跳，最大跳数、每跳三次；无响应不直接判定断路 |
| TCPing | 指定端口 TCP connect，区分连接成功、拒绝、超时 |
| DNS | 系统解析；指定 DNS 的 A/AAAA/CNAME/MX/TXT/NS/PTR/SOA 查询，UDP 截断回退 TCP |
| NTP | 客户端 1～20 次采样、偏差/延迟/stratum、KoD；临时 NTPv3/v4 UDP 服务，支持 IPv4/IPv6、来源 CIDR 和统计；不修改系统时间 |
| iperf3 | Rust 内置 TCP/UDP 测速客户端、服务端；IPv4/IPv6、上传/反向下载、TCP 1–16 并行流、UDP 单流限速、实时速率、抖动/丢包、来源 CIDR；无需安装外部 iperf3 |
| FTP | 被动模式二进制客户端：目录列表、上传、下载；临时 IPv4/IPv6 服务，匿名或账号密码登录、来源 CIDR、默认只读 |
| TFTP | octet/512 字节，上传、下载、临时服务；本地原子提交，默认不覆盖，服务默认只读 |
| Syslog | UDP 发送/接收、优先级提取、实时采样预览、带原始字节的 JSONL、按段轮转 |

GUI 首次启动默认英文，可通过顶部“中文 / English”切换，已保存的语言选择优先沿用。CLI 默认英文，可用 `--lang zh-CN` 切换结果标题。GUI 提供界面风格切换、实时结果、会话任务中心、停止单任务/全部任务、复制和 JSON 导出。GUI 内最多同时运行 4 个任务，保存最近 40 个任务；退出 GUI 会停止其任务。CLI 独立运行，与 GUI 的生命周期互不影响。

顶部“界面风格”可切换“清新”和“视力友好”。视力友好风格增大正文、表单及结果字号，加深辅助文字和状态颜色，并强化输入框边界与键盘焦点；窗口较窄时，配置与结果会改为上下排列。选择立即生效，保存在本机，下次启动沿用；首次使用默认清新风格。

Ping 的“测试包大小 / 字节”支持 0～65500 的整数，默认 32，指 ICMP 数据部分，不包含 IP / ICMP 头。CLI 使用 `--size`，结果及导出 JSON 使用 `payload_size_bytes` 记录该值。大包的实际可达性受网络 MTU、分片和设备策略影响。

## CLI 示例

任务结束后，可在“实时结果”点击“导出图片”保存 PNG，或点击“复制图片”直接粘贴发送。图片保留当前界面的统计卡片、图表、表格、状态、语言与风格；滚动表格会在副本中展开，保留当前筛选条件。JSON 区域、操作按钮和任务中心不进入截图；原页面及滚动位置不变。截图包含界面当前保留的预览，不等于完整 Syslog 日志。

在发布目录的 PowerShell 中执行：

```powershell
.\hopkit-cli.exe ping 127.0.0.1
.\hopkit-cli.exe ping 127.0.0.1 --size 1472
.\hopkit-cli.exe ping ::1 -6 -c 4 --format json
.\hopkit-cli.exe trace 192.168.1.1 --max-hops 15
.\hopkit-cli.exe tcping 192.168.1.1 443 -c 10
.\hopkit-cli.exe dns example.com
.\hopkit-cli.exe dns example.com AAAA -s 192.168.1.1
.\hopkit-cli.exe ntp 192.168.1.10 -c 4
.\hopkit-cli.exe ntp serve --listen 192.168.1.100 --allow 192.168.1.0/24
.\hopkit-cli.exe ntp serve --listen 127.0.0.1 --port 1123 --duration 60 --format jsonl
.\hopkit-cli.exe ftp list 192.168.1.10 /backups --user operator
.\hopkit-cli.exe ftp get 192.168.1.10 /backups/config.cfg -o .\config.cfg
.\hopkit-cli.exe ftp put 192.168.1.10 .\config.cfg --remote-name /backups/config.cfg
.\hopkit-cli.exe ftp serve --root D:\FtpFiles --port 2121 --allow 192.168.1.0/24 --write
.\hopkit-cli.exe tftp get 192.168.1.10 config.cfg -o .\config.cfg
.\hopkit-cli.exe tftp put 192.168.1.10 .\config.cfg
.\hopkit-cli.exe tftp serve --root D:\TftpFiles --listen 192.168.1.100 --allow 192.168.1.0/24
.\hopkit-cli.exe syslog listen --listen 192.168.1.100 --allow 192.168.1.0/24 --dir D:\DeviceLogs
.\hopkit-cli.exe syslog send 192.168.1.10 "HopKit test"
.\hopkit-cli.exe doctor --format json
.\hopkit-cli.exe --help
```

局域网 IP 均为示例，必须替换为实际目标/本机地址。FTP 默认 TCP 21、NTP 默认 UDP 123、TFTP 默认 UDP 69、Syslog 默认 UDP 514；可用 `--port 2121`、`--port 1123`、`--port 1069`、`--port 1514` 测试自定义端口。GUI / CLI 服务默认监听 `0.0.0.0`（所有 IPv4 网卡），对外监听时来源留空表示允许所有来源；可用 `--allow` 指定来源网段，多个 CIDR 用逗号分隔。`--listen` 可指定本机网卡 IP，`::` 表示所有 IPv6 网卡，`127.0.0.1` / `::1` 表示仅本机。

GUI 在 **NTP → 时间服务** 中设置监听地址、端口、允许来源、服务层级和运行时长，然后点击“启动服务”。时长 0 表示运行至手动停止。实时统计显示请求、响应、来源拒绝和无效报文，预览下方显示发送失败次数。“停止”“停止全部”和退出 GUI 都会释放本窗口的服务端口；服务运行时仍可切换到“时间测量”执行客户端测试。

临时 NTP 服务以本机系统时钟为本地参考，默认声明层级 10（`--stratum` 可选 1～15），不校验上游同步、不修改系统时间，精度取决于本机时钟。支持基本 48 字节、无认证的 NTPv3/v4 客户端请求和服务器响应，时间戳布局参照 [RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html#section-8)；不支持认证/NTS、扩展字段、广播、对等和控制模式。请求预览采用采样，计数覆盖全部收到的数据报；取消或限时停止时输出最终 `stopped` 事件。本次源码新增能力不会改变此前发布的二进制包。

### iperf3 网络测速

GUI 左侧 **iperf3** 位于 **TFTP** 正上方。选择“服务端”在本机监听，另一台电脑选择“客户端”并输入服务器 IP；默认端口 5201。上传从客户端发送，下载从服务端发送；两端都可以使用 HopKit，也可与标准 iperf3 对接。整个实现包含在 Rust 核心中，发布包无需附带 iperf3，也不会调用外部程序、动态库或在运行时下载组件。

```sh
hk iperf3 serve --listen 0.0.0.0 --port 5201
hk iperf3 client 192.168.1.10 --duration 10 -P 4
hk iperf3 client 192.168.1.10 --duration 10 -R
hk iperf3 client 192.168.1.10 -u --bitrate-mbps 10 --format json
```

TCP 支持 1–16 流，目标速率 0 为不限速；正数目标速率按每流计算。UDP 当前支持单流、1–10000 Mbps，默认 1200 字节数据报；显示接收端抖动和丢包，结果交换前未知指标显示为 `—`。客户端时长 1–3600 秒；服务端时长 0–86400 秒，0 表示持续运行。服务按客户端请求选择 TCP/UDP，每次接受一个测试；允许来源 CIDR 与其他工具相同。UDP 还需要同端口 TCP 控制连接。每 0.5 秒更新实时速率，停止后保留已传输字节、时长与本端统计；会话历史、重跑和图片/JSON 导出沿用任务中心。

当前不支持 iperf2、认证、SCTP、双向同时传输、预热、按字节/块数结束、UDP 多流、GSO/GRO，以及自定义 MSS/窗口/TOS/拥塞算法等调优参数。服务端收到这些模式会明确拒绝并继续监听。TCP 重传与 CPU 利用率尚未采集，协议中的 CPU 占位值 0 不表示测得零占用；速率结果保留本端及对端原始结果。独立互通验收脚本 `scripts/iperf3-test.py` 可选接收标准 iperf3 路径，该路径仅用于开发测试；原生互测会清空 PATH。

GUI 左侧 **FTP** 位于 **TFTP** 正下方，提供“目录 / 下载 / 上传 / 文件服务”。服务可设置根目录、监听地址和端口、账号密码、允许来源和运行时长；默认匿名只读，勾选后才允许上传新文件，指定非 anonymous 账号时必须设置密码。连接及传输统计、停止、任务中心和 JSON 导出沿用现有流程；序列化请求及结果不包含密码。CLI 优先从 `HOPKIT_FTP_PASSWORD` 读取密码，仅该变量不存在时回退到 `NETBOX_FTP_PASSWORD`，也可用 `--password-env` 指定其他变量名；例如 PowerShell 先设置 `$env:HOPKIT_FTP_PASSWORD='你的密码'` 再使用 `--user operator`。目标参数填写主机名或 IP，不使用含密码的 URL。

`shortcuts/hk.exe` 是 `hopkit-cli.exe` 的同一二进制副本，参数、输出与退出码一致，可直接运行 `.\shortcuts\hk.exe --help`。手动加入 PATH 前先检查已有 `hk`；安装程序不自动修改 PATH，也不覆盖系统命令。新包不再分发 `nb`。

完整兼容策略见 [迁移说明](MIGRATION.md)。新 Windows 安装版与便携 ZIP 均含完整 CLI 和 `hk`；便携单 EXE 仅含 GUI。macOS ZIP 提供两个 CLI 入口，发布 DMG 内置于 `HopKit.app/Contents/Resources/`。

`--format json` 为有限任务单对象；`--format jsonl` 为逐行事件，适用于持续探测和服务。服务 `--duration <秒>` 可指定自动停止，默认 0 表示运行至 Ctrl+C；限时服务也可输出一个最终 JSON。默认英文结果标题，可用 `--lang zh-CN` 切换中文；测试版部分 CLI 帮助/错误仍为中英混合，机器字段固定英文。

退出码：0 达到诊断判据；1 网络/远端结果未达判据；2 参数无效；3 本机权限、占用或能力不足；4 本地读写/输出失败；130 主动取消。Ping/TCPing 至少一次响应为成功，`--strict` 要求零丢包。`status=succeeded` 表示操作执行完成，网络判据仍以 `data.success` 及退出码为准。

## 本测试版边界

- Windows 包交付 x64；macOS 包和运行范围见 `README-macOS.md`。实际验证系统及产物哈希见 `测试报告.md` 和 `validation/`；尚未完成旧 LTSC、Windows 10 最低端、其他架构和真实设备型号认证。
- GUI 是使用已安装 WebView2 的轻量便携包，未附私有 Fixed Version 运行时；CLI 不依赖 WebView2、Node、Python 或开发工具。
- FTP 为明文协议，采用基本 [RFC 959](https://www.rfc-editor.org/rfc/rfc959.html) 命令及 [RFC 2428](https://www.rfc-editor.org/rfc/rfc2428.html) EPSV，IPv4 可回退 PASV；不支持 FTPS/SFTP、主动模式、续传、ASCII 文件转换、删除、重命名或新建目录。服务器最多 4 个并发连接，支持 LIST/NLST/MLSD 和已有子目录，隐藏不安全文件名及符号链接。请选择可信本地根目录，避免其他进程同时更改路径。服务器上传只原子新建、不覆盖；本地下载仅显式启用覆盖且收到成功结束响应后替换。向其他 FTP 服务器上传时是否覆盖由远端决定。目录列表限 10000 项 / 4 MiB；系统分配动态数据端口，防火墙须允许本程序 TCP 数据连接。上传中断后应先核查远端文件再重试；此前发布包不包含本次源码新增功能。
- TFTP 仅 octet，512 字节、一次一个活动传输、根目录内单层文件名；不协商 blksize、不回绕、不支持 netascii/断点续传。最大文件 33,553,919 字节。服务允许上传也只新建，始终不覆盖已有文件。传输端口由 OS 分配，不限制在 v0.3 规划的固定 TID 范围；局域网防火墙需允许本程序的实际 UDP 流量。
- TFTP 每块等待 1 秒、最多重传 5 次，接收端提交后结束等待 2 秒。上传最终 ACK 未收到时，远端文件可能已存在，不能直接假定上传没有发生。用户选择的本地磁盘根目录必须可信。
- Syslog 仅 UDP，接收与文件写入在一个独立工作线程执行，没有正式版的异步队列/SQLite 索引/高级查询。不承诺正式版吞吐量。预览前 10 条后最高约 20 条/秒；UI 可筛选当前 200 条预览，完整接收内容保存在指定目录的 `hopkit-syslog/`。
- Syslog 原始字节以 `raw_hex` 保留；同时保存 UTF-8 容错展示文本。每段 64 MiB，7 天/约 2 GiB 总上限，封段时清理，允许最多一段的临时超额；每秒同步，正常停止再次同步。没有严格崩溃恢复/索引重建保证，强制退出的尾段可能有不完整行。剩余磁盘小于 1 GiB 停止写入。
- ICMP 和 TCP connect 的单次系统调用最久等至所选 timeout，默认 1 秒；取消在当前调用返回后生效。DNS TCP 回退也可能等至读写 timeout，默认不承诺所有参数下取消 P95≤1 秒。
- 不提供 AI/MCP、自动更新、签名更新验证、永久历史、配置导入导出、Syslog TCP/TLS；这些不属于本次基础评估包。未启用遥测，诊断只有用户主动操作才产生相关网络请求。
- 可执行文件未做商业代码签名；应用展示身份使用本地预览命名空间。首次测试版不代表 v0.3 正式版验收已经全部通过。

## 建议首次评估顺序

1. 对 `127.0.0.1` 做 Ping，再对网关做 Ping/Traceroute；检查成功/超时/取消、中文和英文显示。
2. 对已知开放和关闭端口做 TCPing，分别检查连接、拒绝和超时。
3. 比较系统 DNS 与指定企业 DNS 的 A/AAAA 结果；使用可达 NTP 服务器检查时间偏差。
4. TFTP 先在受控目录传输小配置文件，核对文件内容，再连接实际设备。文件服务默认只读，需要备份上传时显式启用新建权限。
5. 指定 UDP 1514 开启 Syslog 接收，通过本机局域网 IP 发送测试消息；核对预览和 JSONL 文件，再按需限制来源网段。
6. 同时运行多个任务，验证停止全部、关闭 GUI 和独立 CLI 的生命周期。

记录问题时提供应用版本、Windows 版本、工具参数、期望/实际结果和导出的 JSON；敏感日志先自行检查与脱敏。

## 开发与复测

当前开发工具链：锁定 Rust 1.96.0、Node 24（`.nvmrc`：24.14.0）。历史 Windows 包使用 Rust 1.94.0、Node 24.3.0、Visual Studio 2022 Build Tools。`Cargo.lock` 和 `package-lock.json` 已锁定依赖。Windows 原包使用 Rust 1.94.0；当前 `rust-toolchain.toml` 为 macOS 扩展锁定到 1.96.0，GitHub 发布构建使用锁定工具链，执行核心测试、本地协议集成测试，以及安装器和 GUI 启动冒烟检查。依赖仍使用锁文件，不要直接随意升级。

```powershell
# 先切换到 Node 24，版本声明见 .nvmrc。
npm ci
npm run build
# 界面风格专项验证（开发环境需提供 puppeteer-core 及浏览器）
# $env:HOPKIT_BROWSER='本机 Chrome/Chromium 可执行文件绝对路径'
# node scripts/appearance-test.mjs
cargo test -p hopkit-core
cargo build --release --features hopkit-desktop/custom-protocol -p hopkit-desktop -p hopkit-cli
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
powershell -NoProfile -File scripts/package.ps1
```

开发 GUI 可运行 `npm run desktop`。集成脚本只使用 Python 标准库，在本机临时端口运行独立协议对端，不依赖公网和真实设备；可在报告中检查逐项证据。GUI 的 CDP 测试只在测试启动时设置 WebView2 调试环境变量，正常发布启动不开放调试端口。

## 源码与反馈

公开源码仓库：[lhuaizhong/HopKit](https://github.com/lhuaizhong/HopKit)。

```sh
git clone https://github.com/lhuaizhong/HopKit.git
cd HopKit
```

Windows 开发步骤见上文，macOS 见 [README-macOS.md](README-macOS.md)。问题和功能建议请提交到 [GitHub Issues](https://github.com/lhuaizhong/HopKit/issues)，分享前移除敏感设备地址、配置和日志内容。构建产物及本地测试文件不纳入 Git 历史。

## 许可证

项目源码采用 [MIT 许可证](LICENSE)，版权所有 2026 Huaizhong Liu。第三方依赖保留各自许可证，详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 和发布包内的依赖声明。

界面底部的 **GitHub · 分享与更新** 链接会在默认浏览器中打开项目仓库，方便分享和获取更新版本。

### 诊断统计与任务（第二阶段修复）

Ping/TCPing 的指标来自后端全程累计，与界面最近预览分开。停止任务后仍可打开、导出已完成部分；首次探测前停止显示“尚无样本”。“已发起探测”包含调用尝试；中断的调用单独记录，耗时与失败率只计算已完成探测。TCPing 显示 TCP 建连耗时与建连失败率。

Traceroute 显示探测跳数、是否到达目标、实际耗时、超时探测数；每跳保留三次探测及各响应地址。TTL 到期表示本跳响应，不代表已到达目标；中间跳超时不能证明链路断开。Trace JSON 使用 `schema_version: 2`，不再提供 Ping 式跨跳丢包率；其余命令保留版本 1，详见 [DIAGNOSTIC-PROTOCOL.md](DIAGNOSTIC-PROTOCOL.md)。CLI 主动取消仍退出 130。

图表使用线性纵轴、ms 刻度和时间轴，失败显示为 ×，悬停查看时间、值及状态；可选最近 20/80/200 个可用样本并筛选。图片保留当前筛选与缩放视图，完整累计指标不随筛选变化。

各工具及操作模式的参数在本次会话中分别保留。使用“沿用上一任务目标”显式传递目标；打开任务不覆盖当前配置，重跑从原请求载入参数，核对后启动。FTP 密码不会进入配置缓存、历史请求或导出，重跑需重新填写。任务中心可筛选本次会话最多 40 项，运行中的任务固定在前且不受历史筛选隐藏，可单独停止。跨重启配置与历史保存属于后续阶段。

### 终端排障主流程（第三阶段，源码新增）

桌面端默认打开“网络排障”：查看本机接口、地址/前缀、网关、DNS 和默认路由，输入故障目标，再查看分阶段证据、可能原因和下一步。网络概览每 15 秒刷新，不自动发公网探测；专业参数折叠，原有独立工具仍在侧栏。

```sh
hk network --format json
hk target 'https://例子.中国:8443/path?token=example' --format json
hk check example.com --format json
hk check 192.168.1.1:443 --timeout-ms 2000 --format json
hk check 'https://example.com/path' --source 192.168.1.10 --format json
hk check example.com --dns-server 192.168.1.1 --trace --format jsonl
```

目标支持 IP、域名、主机:端口、`[IPv6]:端口` 和 HTTP(S) URL。只有主机时不推断 HTTP 服务。中文域名会显示 Punycode 规范化目标。拒绝 URL 用户名密码；请求使用原查询参数，但任务记录、JSON、PNG 与重跑参数移除查询和片段。包含查询参数的重跑需要重新输入原 URL。显式“沿用目标到”只传主机及相关端口，不把 URL 路径传给 Ping。

TCP/HTTP 实际绑定所选源 IP，并记录实际本地/远端地址；系统 DNS、指定 DNS 和系统路由查询不绑定源地址。指定源时跳过暂不支持绑定的 ICMP/Trace；IPv6 scope 使用数字接口索引，带接口名的 scope 和 URL zone identifier 暂不支持执行。选 IP 不保证固定物理出口，实际出口仍受系统路由/VPN 策略影响。

HTTP/TLS 使用编译进程序的 Rust HTTP/1.1、rustls 和内置 Mozilla 根证书，运行时不调用 curl、OpenSSL 或其他第三方程序，不读取 PATH、curl 配置、代理或证书环境变量。默认校验证书；企业内网可通过专业参数或 CLI `--ca-file /path/to/ca.pem` 显式添加 PEM CA（最多 1 MiB），无需关闭校验。附加 CA 文件路径会记录在请求快照中，文件内容不导出。`--insecure-tls` 会醒目标记跳过证书链、主机名和时间校验。

只发 HEAD、不读取正文，完整响应头累计上限 64 KiB，最多 128 个头字段和 8 个临时响应，最多跟随 5 次重定向；拒绝带凭据、非 HTTP(S) 和 HTTPS 降级跳转，不发送 Cookie、Authorization 或 Referer。HEAD 状态可能与 GET 不同。初版固定 HTTP/1.1，不使用系统代理或系统证书库；内置根证书随版本更新，不进行在线证书吊销检查或自动补取缺失中间证书。

结果将任务执行状态与观测分开：`succeeded` 表示检查流程完成，403/500、证书错误、连接拒绝等仍分别保留。`Ctrl+C`/停止返回部分证据，CLI 退出 130。报告时间使用 UTC；卡片阶段总耗时包含适配器开销，协议耗时单独记录。系统 DNS 最多等待 5 秒；每次独立 TCP 尝试和每跳 HTTP（TCP + TLS + 响应）的总过程使用指定超时，候选地址最多 8 个，Trace 最多运行 120 秒。未证明长期可用或故障责任归属。

复测见 [第三阶段测试报告](测试报告-第三阶段终端排障.md)。历史下载包未包含本次源码新增功能；跨平台和多网卡切换实测范围以报告为准。
