# netbox Windows / macOS 基础功能测试版

版本：`0.1.0-alpha.1`。基于上级目录的《netbox 技术方案 v0.3》，实现用于首次操作测试和评估的基础工具集。macOS 的启动、构建、架构和验证边界见 [README-macOS.md](README-macOS.md)；下列 Windows 操作说明继续保留。

## 启动

解压发布包到本地可写目录，双击 `netbox.exe`。GUI 使用系统 Microsoft Edge WebView2 Runtime；通常已随较新 Windows 安装。如果 GUI 无法启动，可先独立使用 `netbox-cli.exe`，再按微软官方说明安装 WebView2 Runtime：<https://developer.microsoft.com/microsoft-edge/webview2/>。

本包不修改 PATH、不安装系统服务、不添加防火墙规则、不自动访问更新服务器。普通用户可执行基本工具；设备访问临时服务时可能需要由用户按本机策略放行入站流量。

## 已实现

| 工具 | GUI / CLI 能力 |
| --- | --- |
| Ping | Windows ICMP API / macOS Darwin ICMP，IPv4/IPv6、次数/持续探测、测试包大小、超时、间隔、延迟和丢包 |
| Traceroute | ICMP 逐跳，最大跳数、每跳三次；无响应不直接判定断路 |
| TCPing | 指定端口 TCP connect，区分连接成功、拒绝、超时 |
| DNS | 系统解析；指定 DNS 的 A/AAAA/CNAME/MX/TXT/NS/PTR/SOA 查询，UDP 截断回退 TCP |
| NTP | 1～20 次采样、偏差/延迟/stratum、KoD；不修改系统时间 |
| TFTP | octet/512 字节，上传、下载、临时服务；本地原子提交，默认不覆盖，服务默认只读 |
| Syslog | UDP 发送/接收、优先级提取、实时采样预览、带原始字节的 JSONL、按段轮转 |

GUI 提供中英切换、实时结果、会话任务中心、停止单任务/全部任务、复制和 JSON 导出。GUI 内最多同时运行 4 个任务，保存最近 40 个任务；退出 GUI 会停止其任务。CLI 独立运行，与 GUI 的生命周期互不影响。

Ping 的“测试包大小 / 字节”支持 0～65500 的整数，默认 32，指 ICMP 数据部分，不包含 IP / ICMP 头。CLI 使用 `--size`，结果及导出 JSON 使用 `payload_size_bytes` 记录该值。大包的实际可达性受网络 MTU、分片和设备策略影响。

## CLI 示例

在发布目录的 PowerShell 中执行：

```powershell
.\netbox-cli.exe ping 127.0.0.1
.\netbox-cli.exe ping 127.0.0.1 --size 1472
.\netbox-cli.exe ping ::1 -6 -c 4 --format json
.\netbox-cli.exe trace 192.168.1.1 --max-hops 15
.\netbox-cli.exe tcping 192.168.1.1 443 -c 10
.\netbox-cli.exe dns example.com
.\netbox-cli.exe dns example.com AAAA -s 192.168.1.1
.\netbox-cli.exe ntp 192.168.1.10 -c 4
.\netbox-cli.exe tftp get 192.168.1.10 config.cfg -o .\config.cfg
.\netbox-cli.exe tftp put 192.168.1.10 .\config.cfg
.\netbox-cli.exe tftp serve --root D:\TftpFiles --listen 192.168.1.100 --allow 192.168.1.0/24
.\netbox-cli.exe syslog listen --listen 192.168.1.100 --allow 192.168.1.0/24 --dir D:\DeviceLogs
.\netbox-cli.exe syslog send 192.168.1.10 "netbox test"
.\netbox-cli.exe doctor --format json
.\netbox-cli.exe --help
```

局域网 IP 均为示例，必须替换为实际目标/本机地址。TFTP 默认 UDP 69、Syslog 默认 UDP 514；可用 `--port 1069`、`--port 1514` 测试自定义端口。GUI / CLI 服务默认监听 `0.0.0.0`（所有 IPv4 网卡），对外监听时来源留空表示允许所有来源；可用 `--allow` 指定来源网段，多个 CIDR 用逗号分隔。`--listen` 可指定本机网卡 IP，`::` 表示所有 IPv6 网卡，`127.0.0.1` / `::1` 表示仅本机。

`shortcuts/nb.exe` 是相同版本的可选短入口，可直接相对调用或自行加入用户 PATH；先检查是否已存在其他 `nb` 命令。完整入口不依赖这个短名。

`--format json` 为有限任务单对象；`--format jsonl` 为逐行事件，适用于持续探测和服务。服务 `--duration <秒>` 可指定自动停止，默认 0 表示运行至 Ctrl+C；限时服务也可输出一个最终 JSON。`--lang en-US` 切换结果标题；测试版部分 CLI 帮助/错误仍为中英混合，机器字段固定英文。

退出码：0 达到诊断判据；1 网络/远端结果未达判据；2 参数无效；3 本机权限、占用或能力不足；4 本地读写/输出失败；130 主动取消。Ping/TCPing 至少一次响应为成功，`--strict` 要求零丢包。`status=succeeded` 表示操作执行完成，网络判据仍以 `data.success` 及退出码为准。

## 本测试版边界

- Windows 包交付 x64；macOS 包和运行范围见 `README-macOS.md`。实际验证系统及产物哈希见 `测试报告.md` 和 `validation/`；尚未完成旧 LTSC、Windows 10 最低端、其他架构和真实设备型号认证。
- GUI 是使用已安装 WebView2 的轻量便携包，未附私有 Fixed Version 运行时；CLI 不依赖 WebView2、Node、Python 或开发工具。
- TFTP 仅 octet，512 字节、一次一个活动传输、根目录内单层文件名；不协商 blksize、不回绕、不支持 netascii/断点续传。最大文件 33,553,919 字节。服务允许上传也只新建，始终不覆盖已有文件。传输端口由 OS 分配，不限制在 v0.3 规划的固定 TID 范围；局域网防火墙需允许本程序的实际 UDP 流量。
- TFTP 每块等待 1 秒、最多重传 5 次，接收端提交后结束等待 2 秒。上传最终 ACK 未收到时，远端文件可能已存在，不能直接假定上传没有发生。用户选择的本地磁盘根目录必须可信。
- Syslog 仅 UDP，接收与文件写入在一个独立工作线程执行，没有正式版的异步队列/SQLite 索引/高级查询。不承诺正式版吞吐量。预览前 10 条后最高约 20 条/秒；UI 可筛选当前 200 条预览，完整接收内容保存在指定目录的 `netbox-syslog/`。
- Syslog 原始字节以 `raw_hex` 保留；同时保存 UTF-8 容错展示文本。每段 64 MiB，7 天/约 2 GiB 总上限，封段时清理，允许最多一段的临时超额；每秒同步，正常停止再次同步。没有严格崩溃恢复/索引重建保证，强制退出的尾段可能有不完整行。剩余磁盘小于 1 GiB 停止写入。
- ICMP 和 TCP connect 的单次系统调用最久等至所选 timeout，默认 1 秒；取消在当前调用返回后生效。DNS TCP 回退也可能等至读写 timeout，默认不承诺所有参数下取消 P95≤1 秒。
- 不提供 AI/MCP、安装器、自动更新、签名更新验证、永久历史、配置导入导出、FTP、Syslog TCP/TLS；这些不属于本次基础评估包。未启用遥测，诊断只有用户主动操作才产生相关网络请求。
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

当前验证工具链：Rust 1.94.0、Node 24.3.0、Visual Studio 2022 Build Tools。`Cargo.lock` 和 `package-lock.json` 已锁定依赖。Windows 原包使用 Rust 1.94.0；当前 `rust-toolchain.toml` 为 macOS 扩展锁定到 1.96.0，Windows 在新工具链上的发布回归尚未执行。依赖仍使用锁文件，不要直接随意升级。

```powershell
npm ci
npm run build
cargo test -p netbox-core
cargo build --release --features netbox-desktop/custom-protocol -p netbox-desktop -p netbox-cli
python scripts/integration_test.py --exe target/release/netbox-cli.exe
powershell -NoProfile -File scripts/package.ps1
```

开发 GUI 可运行 `npm run desktop`。集成脚本只使用 Python 标准库，在本机临时端口运行独立协议对端，不依赖公网和真实设备；可在报告中检查逐项证据。GUI 的 CDP 测试只在测试启动时设置 WebView2 调试环境变量，正常发布启动不开放调试端口。
