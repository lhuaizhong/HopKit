# HopKit 使用指南

[返回 README](../README.zh-CN.md) · [English](usage.md)

本文介绍当前源码的详细用法。安装与快速上手见 README；已发布安装包可能不包含全部新增功能。

[界面与结果](#界面任务与结果) · [多目标监测](#多目标监测) · [CLI](#cli-参考) · [HTTP 文件服务](#http-文件服务) · [Wormhole](#wormhole) · [使用边界](#使用边界) · [验证说明](#验证说明)

## 界面、任务与结果

GUI 首次启动使用英文与“清新”风格。顶部可切换“中文 / English”及“界面风格 → 清新 / 视力友好”，偏好保存在本机。视力友好风格增大字号、加强对比度与焦点提示。CLI 默认英文，`--lang zh-CN` 可切换结果标题；机器字段固定英文，部分测试版帮助与错误信息仍为中英混合。

任务中心最多支持 **4 个并发任务**，保留 **40 项会话任务**；活动任务固定在可筛选历史之前。打开任务不改变当前表单，“重跑”先载入原请求供核对。各工具及操作模式分别保留会话参数。可停止单项或“停止全部”，并保留部分证据。退出 GUI 会停止其任务，独立 CLI 不受影响。任务历史与工具配置不跨重启保存。

Ping/TCPing 按整个任务累计统计，与最近预览分离。中断调用单独计数，不计入延迟与失败率；图表可显示最近 20/80/200 个可用样本，筛选不改变累计指标。Traceroute 按跳报告结果：TTL 到期表示本跳响应，中间跳超时不能单独证明链路断开。

Windows TCPing 优先使用 `SIO_TCP_INFO` 返回的 TCP RTT，排除应用调度和首次调用开销；系统不支持、没有有效 RTT 或发生 SYN 重传时回退到建连耗时。成功样本的 `rtt_source` 标明 `tcp_info` 或 `connect_elapsed`，`connect_elapsed_ms` 始终保留实际建连调用耗时。每次探测使用独立连接，不丢弃首条样本。

## 多目标监测

在 GUI 侧栏选择“多目标监测”（Monitor），设置最多三个 ICMP/TCP 目标，使用共同时间轴查看故障、恢复、备注与前后对比。

```sh
hk monitor --target ping,192.168.1.1 --target ping,example.com --target tcping,business.example,443 --format jsonl
```

目标按系统路由依次探测，每个监测任务占用一个任务名额，与其他工具共用四任务上限。暂停、休眠或采样中断不补造失败样本。两小时以上监测及物理休眠、网卡切换仍待实测。

## CLI 参考

在解压后的目录中运行：

| 平台 | 完整入口 | 等效短入口 |
| --- | --- | --- |
| Windows PowerShell | `.\hopkit-cli.exe` | `.\shortcuts\hk.exe` |
| macOS 终端 | `./hopkit-cli` | `./shortcuts/hk` |

下文统一使用 `hk`，执行时请替换为上表入口；也可先检查命令冲突，再手动将 `shortcuts` 加入 PATH。短入口是完整 CLI 的同一二进制副本，参数、输出与退出码一致。示例中的局域网地址与文件路径请按实际环境替换。

GUI 地址类型统一提供“自动 / IPv4 / IPv6”。CLI 客户端支持 `-4`（`--ipv4`）和 `-6`（`--ipv6`），两者互斥；不指定时采用系统解析顺序。显式选择仅使用对应类型，无匹配地址即报错，不回退到另一类型。Monitor 的选择适用于全部监测目标。DNS 系统解析按地址类型过滤；指定 DNS 时，地址类型限制 DNS 服务器的连接地址，查询记录仍由 A / AAAA 等记录类型决定。

```sh
hk ping example.com -4
hk tcping example.com 443 -4
hk check https://example.com/ -4
hk dns example.com --system -4
```

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

Linux 源码构建的 CLI 支持 IPv4/IPv6 `ping` 和 `trace`，使用内核 ICMP 数据报套接字，无需外部 ping/traceroute 程序。普通用户运行需要系统 `net.ipv4.ping_group_range` 包含进程所属的一个用户组（包括附加组）；权限不足会返回 `PERMISSION_DENIED`。`doctor` 显示 `linux-datagram` 适配器。Linux 的本机网络概览与路由查询尚未实现。

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

默认 `--format text` 面向人工检查：Ping/TCPing 每次探测一行，末尾汇总丢包与延迟；Trace 每跳一行；DNS 列出解析答案；NTP 显示偏差与延迟；iperf3 显示速率及可用的 UDP 丢包、抖动。服务类命令显示监听地址、传输或请求事件及结束统计，不显示内部快照、请求参数、标识符或 JSON 包装。失败探测不显示虚构延迟，Ctrl+C 后保留已有的部分统计。

例如，运行 `hk ping 127.0.0.1 -c 2 --lang zh-CN`：

```text
Ping 127.0.0.1 [127.0.0.1], 32 字节
#1  127.0.0.1  成功  时间=0 ms
#2  127.0.0.1  成功  时间=0 ms

统计: 发送 2, 成功 2, 丢失 0 (0.00%)
延迟 最小/平均/最大 = 0.00/0.00/0.00 ms
```

脚本若依赖结构化字段，请使用 JSON/JSONL；简洁文本排版可能随版本调整。`--lang zh-CN` 切换文本标签，机器字段和退出码保持不变。

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

## HTTP 文件服务

侧栏 **FTP 下一行 → HTTP 文件服务**：选择已有的服务目录，默认监听 `0.0.0.0:80`，可修改端口。点击“启动服务”后可打开或复制网卡访问地址，供其他设备的浏览器使用。同一网络的设备需能访问该 IP 和端口。

浏览器可浏览共享目录及其子目录，点击文件下载。默认只允许下载；启动前勾选“允许文件上传”后，可在网页选择多个文件上传到当前目录，查看进度或取消。上传不覆盖同名文件，未完成文件会被清理。支持中文文件名、IPv4/IPv6、下载范围请求；文件流式读写，不将完整文件载入内存。

可设置来源 CIDR 和运行时长（0 表示持续运行）。停止服务或退出 HopKit 会关闭监听和活动传输，保留完成的传输统计。服务为临时明文 HTTP，不提供登录认证或 HTTPS；仅共享用户选择的目录，以点开头的文件和符号链接不开放。macOS 的 80 端口如受权限限制，可改为 8080；端口占用时会显示启动错误。

```sh
hk http serve --root ./shared
hk http serve --root ./shared --port 8080 --write
hk http serve --root ./shared --listen :: --port 8080 --allow ::1/128 --duration 300 --format jsonl
```

## Wormhole

Wormhole 通过一次性配对码传输单个普通文件。在侧栏选择 **Wormhole** 进行发送或接收；接收方选择保存位置后显式确认。已有文件不会被覆盖。来自 Python 对端的目录传输以 ZIP 原样保存，不自动解压。双方须保持在线，不支持续传或离线投递。

```sh
hk wormhole send ./report.zip
hk wormhole receive --dir ./received
hk wormhole receive --code-stdin --dir ./received --yes --format jsonl
hk wormhole send ./report.zip --connection relay --format jsonl --emit-code
```

接收目录必须已存在。交互接收会提示输入配对码并确认；JSON/JSONL 接收必须显式指定 `--yes`，可用 `--code-stdin` 从标准输入读取配对码，避免作为命令行参数传入。`--verify` 仅适用于交互文本终端。

### 连接与隐私

- 默认使用公共 WSS Mailbox 和 Transit Relay，仅在开始传输后联网，支持自定义服务。
- `--connection auto` 自动选择连接；`direct` 仅直连传输数据，仍需信令；`relay` 仅中继，不使用 STUN 或直连监听。
- 文件数据端到端加密，但信令、中继和 STUN 服务仍可观察连接元数据；直连双方会获知彼此地址。
- 接收配对码不进入历史或导出，重跑需重新配对。机器输出默认不含配对码；发送码写入 stderr，仅发送端 JSONL 可通过 `--emit-code` 主动输出一次敏感码。
- Wormhole 使用系统原生 TLS 信任库，与 HTTP 排障不同；不提供跳过 TLS 校验的选项。

### 传输中断

正常取消会清理未提交的 `.hopkit-*.part` 文件。强制退出或断电可能在目标旁留下临时文件，请在传输停止后手动清理。若文件已经提交，即使回执发送失败也会保留。

本功能仍待发布验收。已执行与待完成的检查见[实现记录](../WORMHOLE_IMPLEMENTATION.md)，EUPL-1.2 分发义务见[分发许可说明](../DISTRIBUTION_LICENSES.md)。

## 使用边界

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
| HTTP | TCP 80 | 8080 |
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

## 验证说明

共用协议集成脚本使用 Python 标准库与独立本地对端，不依赖公网。构建 Release CLI 后，Windows 可在项目根目录运行：

```powershell
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
```

macOS 将程序路径替换为 `target/release/hopkit-cli`。专项检查包括 `scripts/diagnostic-trust-test.py`、`scripts/iperf3-test.py`、`scripts/check-test.py`，运行前需核对平台与开发依赖。Wormhole 测试前提与发布验收要求见[发布说明](../RELEASING.md)。

运行 `python scripts/cli-text-test.py --exe target/release/hopkit-cli.exe` 可检查本地 CLI 输出（macOS 去掉 `.exe`），覆盖简洁文本、中英文及 JSON/JSONL 兼容性，不访问公网。

UI 测试使用浏览器 fixture 或测试专用调试，正式启动不开放调试端口。本地证据写入 `test-output/`，包内证据如有提供则位于 `validation/`，路径均相对于项目或安装包根目录。结论仅适用于记录的平台与产物；其他架构、最低系统版本、真实设备及多网卡/VPN 动态环境需单独验证。
