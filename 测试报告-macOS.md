# netbox macOS 基础测试版验证记录

日期：2026-10-27（Asia/Shanghai）。版本：`0.1.0-alpha.1`。此次交付范围为与 Windows 基础版对应的 GUI / CLI 七类工具，不是《技术方案 v0.3》D3 正式版验收。

## 环境与产物

| 项目 | 实际记录 |
| --- | --- |
| 运行验证机器 | Apple Silicon arm64，macOS 27.0.1 / 26A434，普通用户 UID 501 |
| 工具链 | Rust 1.96.0，Node 25.5.0，macOS SDK 27.0 |
| GUI | Tauri 2.12.5 + Vue 3，系统 WKWebView，本地资源 |
| 原生目标 | `aarch64-apple-darwin`，deployment target 11.0 |
| Intel 目标 | `x86_64-apple-darwin`，deployment target 10.15；仅交叉构建、架构及签名检查 |
| 签名 | GUI / CLI 为 ad-hoc，未做 Developer ID / Apple 公证 |
| 发布目录 | `release/netbox-0.1.0-alpha.1-macos-arm64/`、`release/netbox-0.1.0-alpha.1-macos-x86_64/` |

最终包内 `BUILD_INFO.json` 记录源提交、工作区修改状态、源文件摘要、Rust/Node/SDK、依赖锁摘要和所交付二进制的 SHA-256。包内另有逐文件 `SHA256SUMS.txt`，ZIP / CLI 压缩包旁有 `.sha256` 文件。SHA-256 不代替发布者签名。

## 已执行结果

| 验证 | 结果与证据 |
| --- | --- |
| 核心单元测试 | 14/14 通过，包含 macOS ICMP Echo/TTL 引用报文校验、忽略自身 IPv6 请求与错误任务、畸形长度、可变包大小与完整回包校验、校验和；以及 DNS/TFTP/Syslog 边界、默认对外监听与显式来源限制 |
| 生命周期测试 | 5/5 通过：Syslog 停止释放 socket/目录锁、TFTP 停止、长间隔探测取消、重复目录锁拒绝、未完成下载清理临时文件 |
| 前端 | `vue-tsc --noEmit` + Vite 生产构建通过 |
| 静态检查 | `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`git diff --check` 通过 |
| 独立协议互通 | 16/16 通过。Python 标准库独立对端验证 ICMP 双栈、Trace、系统 DNS、TCP 建连/拒绝、DNS UDP/TCP 回退/NXDOMAIN、NTP 偏差/KoD、TFTP 0/512/513/4097 字节边界及上传/服务/拒绝覆盖、Syslog 原始字节与目录独占 |
| 并发 ICMP | 8 个 CLI（IPv4/IPv6 各 4 个）共 64 个 Echo 响应，无丢失、无任务串包；IPv6 Trace 到 `::1` 成功 |
| Ping 包大小 | IPv4/IPv6 分别测试 0、1、32、1472、4096、65500 字节，独立 ICMP socket 观测回包长度分别为数据长度 + 8 字节 ICMP 头；默认 32，65501、负数、小数被拒绝。macOS 专项脚本共 8/8 通过，新增默认对外服务验证 |
| Ctrl+C | ICMP 单次预算 10 秒，发送 Ctrl+C 后低于 1 秒结束，退出码 130；一次实测 21ms，以最终 JSON 中测量值为准 |
| 断管 | 持续 Ping 的 stdout 关闭后结束，退出码 4 |
| 双栈临时服务 | 高端口 TFTP/Syslog 的 IPv4 和 IPv6 收发、中文/空格路径、文件内容、停止后端口重新绑定均通过 |
| 标准端口 | 本次 UID 501 复测 UDP 69 和 UDP 514 均可绑定；早期受限运行环境曾返回 `PERMISSION_DENIED`。当前结果见最终专项 JSON，不能保证其他环境权限或端口占用情况；没有修改权限或自动切换端口 |
| 解压与移动 | ZIP 及独立 CLI 压缩包解压到中文/空格路径后签名保持有效；独立短入口 doctor 和移动后的 `.app` 实际 Ping 1/1 成功 |
| 签名与依赖 | `codesign --verify --deep --strict` 检查 GUI，`--verify --strict` 检查 CLI；Mach-O CPU 与目标匹配；CLI 只依赖 macOS 系统动态库 |

机器可读结果位于 `test-output/integration-release-macos-arm64.json`、`test-output/platform-macos-arm64.json`，发布时只收录与该 CLI SHA-256 相同的结果。macOS 专用脚本中“标准端口”项目验证的是权限观测与错误行为，不将拒绝结果解释为服务可用。Intel 包不附 ARM 运行报告。

## GUI 操作验证

使用实际 `.app` 窗口执行并读取原生可访问性树及截图，未使用浏览器模拟替代 WKWebView。首版已完成以下基础验证；后续专项复测列于下方，当前包内 GUI 报告只记录与当前二进制匹配的最新专项结果：

- 正确显示 `macOS Apple Silicon`、版本及预览标识；中文/英文切换。
- GUI 对 `127.0.0.1` Ping，4 发 4 收，丢包率 0；任务中心完成状态正确。
- 系统 DNS 查询 `localhost`，返回 `::1`、`127.0.0.1`，`source=system`，TTL/RCODE 为 null。
- Syslog 通过 GUI 在 `127.0.0.1:15514` 启动，独立 Python socket 发送包含 `<script>alert(1)</script>` 和中文的报文；界面作为字面文本显示，接收/写入各 1，原始日志内容核对一致。
- 从 GUI 停止 Syslog，结果报告取消且文件同步；原生保存对话框导出 JSON，重新读取文件验证 `status=cancelled` 及结果结构。
- 窗口关闭按 macOS 临时工具语义退出；有活动任务时提供停止并退出/取消对话框，确认后停止该 GUI 任务，不影响独立 CLI。

原始开发验证截图和导出文件保留在 `test-output/`；发布材料会替换本机绝对路径。截图和交互细节只证明已执行场景，不能推导所有 GUI 控件或性能指标均已覆盖。

### Ping 测试包大小专项复测（2026-10-27）

- 默认输入值为 32，范围提示为 0～65500 字节，明确仅计算 ICMP 数据；只在 Ping 配置中显示，中文/英文标签均正确。
- GUI 以 1472 字节对 `127.0.0.1` 执行 4 次测试，4 发 4 收；4096、0、65500 字节对 `::1` 分别执行 1 次测试，均成功。
- 输入 65501 后点击开始，WKWebView 显示上限验证提示，没有创建新任务。
- 结果区及结构化结果包含本次 `payload_size_bytes`。将配置改为 65500 后重新打开 1472 字节历史任务，仍显示 1472，未随表单变化。
- 使用原生保存对话框导出 1472 字节任务，重新读取文件核对汇总及每个 sample 的大小和 4 个响应，全部一致。
- 本次重新执行 13 项核心单元测试、5 项生命周期测试、16 项协议互通和 7 项 macOS 专项，均通过；生产前端构建、fmt、Clippy、差异空白检查均通过。

专项证据：`test-output/gui-macos-arm64.json`、`test-output/gui-ping-size-ax.json`、`test-output/gui-ping-1472-macos.png`、`test-output/ping-size-gui-1472.json`。此项大包测试使用本机回环，未据此宣称真实网络 MTU 或物理设备大包互通已通过。Windows 分支同步支持大小及回复缓冲区调整；本次未在 Windows 实机复测或重打 Windows 包。

### 服务启动按钮修复专项复测（2026-10-27）

- TFTP 文件服务、Syslog 接收服务启动后，“启动服务”禁用并显示“服务运行中”；停止中继续禁用，停止完成或启动失败后恢复。判断使用任务类型和操作模式，不将客户端发送/传输误判为服务。
- 原生窗口双击启动 TFTP / Syslog，各只创建一个服务；再次点击禁用按钮不会创建失败任务。切换工具页再返回服务模式后，禁用状态及“停止”入口保持正确。
- Syslog 运行时切换发送模式可正常发送，返回接收模式后仍阻止重复启动；停止按钮正确停止接收服务，日志实际收到并写入 1 条消息，随后可重新启动。
- TFTP 服务运行时下载模式可用；独立 Python UDP 客户端执行 RRQ 成功取得 33 字节配置文件。切换页面后停止原服务成功，修正一次无效目录造成的启动失败后可再次启动。
- Syslog 标准端口权限失败后按钮恢复；改用高端口后可与 TFTP 同时运行。中文/英文运行状态均正确；“停止全部”后两个服务按钮恢复，UDP 15669 / 15514 均能重新绑定。
- 原生 GUI 专项记录共 16 项通过；前端生产构建、工作区 Clippy、fmt 和差异空白检查通过。协议核心与 CLI 未修改，既有与 CLI SHA-256 匹配的互通记录继续随包收录；本次未复跑完整协议套件。

最新专项证据为 `test-output/gui-macos-arm64.json`、`test-output/gui-service-buttons-ax.json`、`test-output/gui-service-buttons-macos.png`。此前 GUI 报告与旧包保留在 `test-output/before-service-button-fix-*`。Intel 已重新构建，仍未做 Intel 实机测试；Windows GUI 共用此次修复代码，但未重打 Windows 包。

### Traceroute 每跳一行专项复测（2026-10-27）

- 路由追踪使用“跳数 / 响应地址 / 探测 1 / 探测 2 / 探测 3”五列表格，按 hop 合并、按 seq 放入三次探测槽位，每次保留延迟和状态。超时显示 `*` 及“超时”，不把未响应的目标地址当作路由器地址；同跳多响应地址保留各探测的地址对应关系。
- 原生 macOS GUI 验证 IPv4 / IPv6 回环的三次响应均只显示一行；真实网络前三跳九次 TTL 到期响应显示三行，六跳十八次结果显示六行，含同跳两次超时、一次响应的混合结果及整跳超时结果。
- 按响应地址筛选时保留整跳三次探测；再次打开历史任务仍按跳合并；中文/英文表头与状态正确。原生保存对话框导出后独立读取 JSON，确认九次原始 samples 的 hop、seq、地址和 RTT 均保留。
- Ping 实测仍使用原来的逐次结果表格。前端类型检查、Vite 生产构建、差异空白检查通过。
- 另执行 8 项分组数据边界检查：乱序序号、TTL 响应、超时地址排除、同跳多地址、零 RTT / IPv6、未完成槽位、整跳筛选、64 跳 192 个汇总样本及空输入。此处的多响应地址与 64 跳是数据边界检查，不冒充真实网络实测。

当前 GUI 报告 `test-output/gui-macos-arm64.json` 更新为此次专项结果；原始界面记录及截图为 `test-output/gui-trace-rows-ax.json`、`test-output/gui-trace-one-hop-macos.png`、`test-output/gui-trace-three-hops-macos.png`、`test-output/gui-trace-mixed-timeouts-macos.png`，分组边界记录为 `test-output/trace-grouping-checks.json`，原始导出为 `test-output/trace-three-hops-gui.json`。先前报告及包保留在 `test-output/before-trace-row-update-*`。本次只修改前端展示与说明，协议核心及 CLI 未变；Intel 包重新构建但未实机运行，Windows 包未重打。

### 默认对外监听专项复测（2026-10-27）

- GUI、CLI 和共享请求默认监听均改为 `0.0.0.0`（所有 IPv4 网卡）。对外监听时来源 CIDR 留空允许所有来源；填写网段后按 CIDR 限制。手动指定 `127.0.0.1` / `::1` 保持本机服务，`::` 支持所有 IPv6 网卡，中文/英文帮助同步更新。
- 独立 Python 客户端通过本机非回环 IPv4 验证 CLI 默认服务（不传 `--listen` / `--allow`）：TFTP 下载内容一致、Syslog 写入一条日志；显式 IPv6 `::` 监听亦通过，停止后端口释放。
- 实际 macOS GUI 未修改默认监听及空来源，分别启动 TFTP / Syslog，任务目标显示 `0.0.0.0`；通过本机局域网 IP 成功下载 27 字节文件并收到/写入一条日志。两个服务同时运行时启动按钮保持禁用；停止全部后均结束。
- GUI 在 `0.0.0.0` 下填写 `127.0.0.0/8` 后，局域网来源被拒绝、本机回环被接收：收到 2、写入 1、来源拒绝 1；独立核对日志文件仅包含放行消息，停止后端口可重新绑定。
- 核心单元测试 14/14、生命周期测试 5/5、协议集成 16/16、macOS 专项 8/8、GUI 专项 8/8 均通过；前端生产构建、fmt、Clippy 与差异空白检查通过。ARM 和 Intel 包已重新构建；Intel 仅交叉构建及签名/架构检查，未做 Intel 实机测试，Windows 包未重打。

当前二进制对应报告为 `test-output/gui-macos-arm64.json`、`test-output/platform-macos-arm64.json`、`test-output/integration-release-macos-arm64.json`；界面原始记录为 `test-output/gui-outward-defaults-ax.json`，截图为 `test-output/gui-outward-tftp-macos.png`、`test-output/gui-outward-filter-macos.png`。本次局域网 IP 测试在同一台 Mac 上使用独立协议客户端，未使用第二台物理设备。此前包及报告保留在 `test-output/before-outward-default-*`。

### GitHub 项目入口（2026-10-28）

- 所有工具页共用的界面底部新增 GitHub 项目链接，中文显示“分享与更新”，英文显示“Share & updates”，目标固定为 `https://github.com/lhuaizhong/netbox`。
- 链接通过桌面后端调用系统默认浏览器，打开失败时保留可复制的项目地址；不替换工具窗口，也不自动检查或下载更新。
- 前端类型检查与生产构建、桌面后端 Clippy 通过。macOS 原生窗口验证中英文入口及实际浏览器跳转；Windows 代码共用，但本次未在 Windows 实机验证或重打 Windows 包。

## 平台差异与待验证项

1. macOS 使用无特权 ICMP 数据报套接字，不运行系统 ping/traceroute，不安装提权 helper。每次收包验证 id、sequence、目标及 Echo 随机载荷，引用错误报文验证原目标和标识；收包每 50ms 检查取消。
2. 标准服务端口可用性随运行环境与占用情况变化，本次 UID 501 复测 UDP 69 / 514 可用，早期受限环境曾拒绝。权限不足时可手动选择 TFTP 1069 / Syslog 1514；GUI 不提权，尚未实现最小特权辅助进程，不用高端口通过替代其他环境的标准端口验收。
3. Intel 实机、macOS 10.15 / 11 最低端、真实设备、VPN/多网卡、链路本地 scope、休眠恢复、两小时压力/轮转与灾难恢复、真实网络下载 quarantine 首次放行均未完成。交叉编译与 Mach-O deployment target 不是运行兼容证明。
4. 未将 CLI 通过推导成全部 GUI 互通通过，未将本机协议对端通过推导成特定路由器/交换机固件认证。未在当前 Mac 复跑 Windows 运行测试；Windows ICMP 实现保留原平台条件分支。
5. TFTP 仅 octet / 512 字节 / 单活动传输，最大 33,553,919 字节；Syslog 仅 UDP / JSONL。AI/MCP、Syslog TCP、索引恢复、自动更新和更新签名等保持原基础版缺项，详见 `README-macOS.md`。
