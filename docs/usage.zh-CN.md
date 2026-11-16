# HopKit 使用指南

[返回 README](../README.zh-CN.md) · [English](usage.md)

本文介绍当前源码的详细用法。安装与快速上手见 README；已发布安装包可能不包含全部新增功能。

`0.2.0-dev.5` 新增专家工具 TCP/UDP 调试助手、小工具中的本机端口与连接及 Wake-on-LAN。用法、正文记录/导出、会话和字节预算、CLI stdin 格式、平台限制见 [M4 开发与验收记录](M4-开发与验收记录.md)。WOL 发送完成不代表设备已唤醒；连接查询受权限影响；收发正文默认不进入通用任务导出。

[界面与结果](#界面任务与结果) · [多目标监测](#多目标监测) · [CLI](#cli-参考) · [HTTP 文件服务](#http-文件服务) · [Wormhole](#wormhole) · [使用边界](#使用边界) · [验证说明](#验证说明)

## 界面、任务与结果

GUI 首次启动根据系统语言选择中文或英文，使用“清新”风格。顶部可切换“中文 / English”及“界面风格 → 清新 / 视力友好”，偏好保存在本机。视力友好风格增大字号、加强对比度与焦点提示。CLI 默认英文，`--lang zh-CN` 可切换结果标题；机器字段固定英文，部分测试版帮助与错误信息仍为中英混合。

任务中心最多支持 **4 个并发任务**，保留 **40 项会话任务**；活动任务固定在可筛选历史之前。打开任务不改变当前表单，“重跑”先载入原请求供核对。各工具及操作模式分别保留会话参数。可停止单项或“停止全部”，并保留部分证据。退出 GUI 会停止其任务，独立 CLI 不受影响。任务历史不跨重启保存，也不自动重启任务。有效的无秘密工具参数按工具/模式保存在本机并恢复。

侧栏依次为“常用工具 / 文件传输 / 专家工具 / 小工具”。常用工具保留 7 个日常诊断工具；文件传输包含 TFTP、FTP、HTTP 文件服务和 Wormhole，FTP 与 HTTP 相邻；TCP Scan、Discovery、TCP/UDP 调试、多目标持续监测和 Syslog 临时日志位于专家工具；子网计算、本机连接和 Wake-on-LAN 位于小工具。侧栏不再显示底部预览标识、版本号和本地优先提示，工具列表占满剩余高度并可独立滚动。点击分类会选中该分类中的工具。顶部搜索覆盖所有分类，支持中英文名称、协议和别名，精确名称优先；Enter 选择首项，Esc 清空。星标将工具加入全局收藏，再次点击取消。分类、最近工具和侧栏滚动位置在本机保存。旧导航记录会按工具 ID 自动迁入新分类，保留收藏与参数。切换分类不启动或停止任务；侧栏“任务中心”始终可访问活动任务。

展开“收藏目标与现场配置”，填写名称后保存当前目标或配置，点击已保存条目手动载入。目标收藏进入网络排障首页，配置恢复对应工具和操作模式。删除按钮移除条目。导出为 JSON；导入按条目 ID 合并，工具/模式相同的设置以导入值为准。库使用 schema 1，文件和合并后总大小最多 256 KiB，目标、命名配置和工具模式配置各最多 100 项。无效文件不会替换当前库。存储受限时显示错误，当前会话仍可使用。

FTP 密码、Wormhole 配对码、Syslog 报文正文、URL 查询参数与片段不保存。URL 用户名/密码会被拒绝。普通路径、目标地址和 FTP 用户名会保存在配置中，导出前可自行检查。恢复配置后需要重新输入凭据；所有保存、载入和导入操作均不自动启动网络任务或服务。持久历史、HTML 报告及选择性脱敏将在后续里程碑交付。

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

### 设备发现（M3）

```sh
hk discover                                      # 仅读邻居缓存，不发送探测
hk discover 192.168.1.0/28 --interface en0 --methods arp,icmp,tcp --ports 80,443
hk discover 192.168.1.10-192.168.1.20 192.168.2.5 --methods tcp --format json
hk discover fe80::1%4 --methods icmp
hk discover 192.168.1.10 --name-lookup --dns-server 192.168.1.1 --format jsonl
```

接口标识：macOS/Linux 使用接口名；Windows 使用接口别名或数字 ifIndex。GUI 的“刷新接口”只查本机，“填入此网段”只填写参数，点击开始才探测。目标留空仅读缓存；IPv6 仅明确地址，不枚举前缀。最多 4096 个候选、8 个 TCP 端口、65536 个组合；单次超时 100～1000 ms。发现与扫描共用每进程 128 个许可、200 次/秒预算，每任务最大 100 次/秒。

结果分别展示本次响应、邻居缓存、PTR 名称来源与时间。TCP 拒绝也属于响应证据，可能来自中间设备；未响应不代表离线或空闲 IP，缓存不代表本次在线。MAC 变化只提示排查线索，不判定冲突；跨路由地址不会继承网关 MAC。系统隐藏或占位 MAC 记为未知，不推断厂商。ARP 默认与 ICMP/TCP 一起启用，只对本机接口网段内且实际路由直连的 IPv4 目标发起；经网关或 VPN 隧道的目标跳过。ARP 响应可发现禁 Ping 设备并携带其 MAC；结果页只显示响应设备、按 IP 数值排序、直接展示 MAC，JSON 保留未响应候选。macOS 主动 ARP 需要 BPF 读写权限和可见的接口 MAC，权限不足会明确报错并继续 ICMP/TCP；可用 `--methods icmp,tcp` 关闭 ARP。主动 ND、RFC 5227 Probe 和 OUI 数据未提供。

接口选择用于源 IP 与 IPv6 scope，不强制固定接口路由；PTR 使用到明确指定 DNS 服务器的系统路由，最多 4 并发、UDP 有界超时。响应先出现，名称查询失败不抹去响应。结果可手动传给扫描、Ping、TCPing、网络排障/Web；适用源 IP 和 scope 保留，不自动运行。源 IP/接口不在重启时自动绑定，显式载入现场配置仍可恢复。

CLI 正常完成（包括零响应）退出 0，参数错误 2，本机探测/能力错误 3，取消 130 并保留部分结果。GUI JSON 导出完整任务；截图导出当前筛选/分页的可见结果。平台实测边界见 [M3 开发与验收记录](M3-开发与验收记录.md)。

设备发现按 IP 保留已响应设备，不随实时事件队列淘汰而消失。首轮 ICMP 超时且其他方法也未响应的目标，在首轮完成后仅追加一次受限 ICMP 复核；超时本身仍不视为响应。

### TCP connect 端口扫描（M2）

```sh
hk scan tcp 192.168.1.10 --ports 22,80,443,8000-8010 --concurrency 32 --rate 100
hk scan tcp 192.168.1.0/28 192.168.2.10-192.168.2.12 --ports 22,443 -4 --format json
hk scan tcp example.com --ports 443 --timeout-ms 2000 --source 192.168.1.10 --format jsonl
hk scan tcp ::1 --ports 22,443 -6
```

目标用多个位置参数传入，支持 IP、域名、IPv4 CIDR 和完整起止地址；IPv6 仅接受明确地址，数字 scope 如 `fe80::1%4`。域名只解析一次，候选地址在目标事件和最终结果中展示，所有候选按 IP 分列。普通 IPv4 CIDR 排除网络/广播，`/31`、`/32` 保留全部地址。重复地址和端口会去重。

最多 4,096 个候选、65,536 个 IP×端口组合。并发默认 32、最高 128；速率默认/最高每任务 100 次/秒，同一 GUI/进程合计最多 128 个扫描连接、200 次/秒。超时为 100～10,000 ms；独立 CLI 进程间不共享预算。源地址默认留空，由系统路由选择默认网络地址；应用启动时不自动恢复上次的源绑定。需要固定出口时，可手动填写本机数字 IP 或主动载入现场配置，且须与全部候选类型一致；“使用默认网络地址”按钮可清空绑定。

GUI 选择“专家工具 → TCP Scan”，预设仅表示常见端口用途。状态包括开放、拒绝、超时、不可达、本机错误与中断，保留实际端点和错误证据；超时不证明 filtered，端口号不证明实际服务身份。没有开放端口仍可成功完成扫描。结果每页 100 项，筛选/翻页不改变累计数据；“导出 JSON”导出完整结果。开放行的“TCPing →”仅预填目标和端口，需手动开始。原 TCPing 不支持扫描源绑定，联动时会提示。

停止或 Ctrl+C 返回已完成结果、剩余数量与 `partial=true`（退出 130）；解析阶段停止时总数和剩余数未知，使用 null，不启动探测。实时预览有容量限制，完成后使用完整结果分页。

### 离线 IPv4/IPv6 子网计算（M2）

```sh
hk subnet inspect 192.168.10.25/24 --format json
hk subnet inspect 192.168.10.25/255.255.255.0
hk subnet inspect ::/0 --format json
hk subnet contains 192.168.10.0/24 192.168.10.25
hk subnet overlap 192.168.10.0/24 192.168.10.128/25
hk subnet split 192.168.10.0/24 --prefix 26
hk subnet aggregate 192.168.10.0/25 192.168.10.128/25 --format json
```

GUI 选择“小工具 → Subnet”。网段信息用摘要卡片展示标准化网络、地址数量和主机数量，并列出地址/主机范围、掩码及广播语义。包含与重叠操作直接显示判断结论；拆分和聚合展示网段数、每个网段的地址数量及分页列表（每页 50 项）。原始 JSON 默认折叠，可展开查看，继续支持复制输入和结果及完整 JSON 导出。全程离线，不占网络任务名额；四个网络任务运行时也能计算。`contains` 真退出 0、假退出 1，参数错误退出 2。

IPv4 接受连续点分掩码；普通子网给出常见主机范围，`/31` 按点到点双地址、`/32` 按单地址解释，实际可用性取决于配置。IPv6 无广播，不扣首尾地址。地址数量以精确十进制字符串输出，IPv6 `/0` 也不丢失精度。拆分结果与聚合输入最多 1,024 项；精确聚合保留空洞，不扩大覆盖范围。机器字段固定英文，默认文本输出保留结构化计算结果。

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

## MTR 持续路由诊断（dev.6）

专家工具中搜索 MTR，填写目标后启动。支持 ICMP IPv4/IPv6、源 IP、轮数/时长、轮次间隔、最大 TTL 与每秒探测上限。默认 100 轮、1 秒间隔、30 跳、单探测 1 秒、20 次/秒；所有扫描/发现/MTR 在同一 GUI 进程合计最多 200 次/秒，慢轮次不排队。停止保留统计；切换分类后可在任务中心停止。

```sh
hk mtr example.com --cycles 100 --interval-ms 1000 --max-hops 30 --format jsonl
hk mtr ::1 -6 --source ::1 --cycles 10 --format json
hk mtr 192.0.2.1 --cycles 0 --duration 60 --ptr-server 192.0.2.53 --format jsonl
```

PTR 默认关闭，显式填写解析器才查询；使用独立 UDP、系统路由、300 ms 超时，不跟随 ICMP 源绑定。名称不证明身份。目标到达率按完整轮次统计；中间跳未回应率不等于转发丢包率。RTT 抖动是同 TTL/响应者连续有效 RTT 差绝对值的均值，无相邻对显示 —；超时、缺口、响应者变化断开计算。

表格中的响应地址可展开各响应者统计；路径变化与缺口另有时间线。最近 256 个样本/变化/缺口、每 TTL 最多 16 个响应者，累计计数独立保留；JSON 导出说明预览淘汰。JSON 仅支持有限运行，JSONL 为样本/计数/最终结果；0 表示至少一轮到达目标，1 表示完成但未到达，2 参数错误，3 本机能力/权限，130 主动取消。`--strict` 要求所有完整轮次到达目标，不使用中间跳未回应率判失败。

当前真实隔离多跳、macOS 原生及物理休眠仍待现场验收，见 [M5 记录](M5-开发与验收记录.md)。

## SNMP（M6 源码开发版）

专家工具 → SNMP 提供 v2c/v3 只读 GET、GETNEXT、GETBULK、WALK、设备概览及 IF-MIB 接口观察。填写运行凭据后手动启动；现场配置和导出排除 community/认证/加密密码。Community 在当前页面保留，方便重复查询；v3 密码启动后清空。支持数值 OID 与内置常用名称，没有完整 MIB 编译器或 SET。

```sh
hk snmp get 192.0.2.10 sysName sysUpTime --community-env HOPKIT_SNMP_COMMUNITY
hk snmp walk 192.0.2.10 system --format json
hk snmp interfaces 192.0.2.10 --snmp-version 3 --username operator --samples 0 --format jsonl
```

按版本在受控进程环境中设置 `HOPKIT_SNMP_COMMUNITY`、`HOPKIT_SNMP_AUTH_PASSWORD`、`HOPKIT_SNMP_PRIVACY_PASSWORD`，不把明文写进 CLI 参数。v3 默认 SHA-256/AES-128 authPriv，可显式选择 SHA-1 兼容或其他安全级别。Counter64 以十进制字符串返回；缺口、重启和断点使速率为空。范围、限制及平台/真实设备/长时验收见 [M6 记录](M6-开发与验收记录.md)。
