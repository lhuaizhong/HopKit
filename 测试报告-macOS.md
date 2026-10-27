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
| 核心单元测试 | 13/13 通过，包含 macOS ICMP Echo/TTL 引用报文校验、忽略自身 IPv6 请求与错误任务、畸形长度、可变包大小与完整回包校验、校验和；以及现有 DNS/TFTP/Syslog 边界 |
| 生命周期测试 | 5/5 通过：Syslog 停止释放 socket/目录锁、TFTP 停止、长间隔探测取消、重复目录锁拒绝、未完成下载清理临时文件 |
| 前端 | `vue-tsc --noEmit` + Vite 生产构建通过 |
| 静态检查 | `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`git diff --check` 通过 |
| 独立协议互通 | 16/16 通过。Python 标准库独立对端验证 ICMP 双栈、Trace、系统 DNS、TCP 建连/拒绝、DNS UDP/TCP 回退/NXDOMAIN、NTP 偏差/KoD、TFTP 0/512/513/4097 字节边界及上传/服务/拒绝覆盖、Syslog 原始字节与目录独占 |
| 并发 ICMP | 8 个 CLI（IPv4/IPv6 各 4 个）共 64 个 Echo 响应，无丢失、无任务串包；IPv6 Trace 到 `::1` 成功 |
| Ping 包大小 | IPv4/IPv6 分别测试 0、1、32、1472、4096、65500 字节，独立 ICMP socket 观测回包长度分别为数据长度 + 8 字节 ICMP 头；默认 32，65501、负数、小数被拒绝。macOS 专项脚本共 7/7 通过 |
| Ctrl+C | ICMP 单次预算 10 秒，发送 Ctrl+C 后低于 1 秒结束，退出码 130；一次实测 21ms，以最终 JSON 中测量值为准 |
| 断管 | 持续 Ping 的 stdout 关闭后结束，退出码 4 |
| 双栈临时服务 | 高端口 TFTP/Syslog 的 IPv4 和 IPv6 收发、中文/空格路径、文件内容、停止后端口重新绑定均通过 |
| 标准端口 | **普通用户绑定 UDP 69 和 UDP 514 均被系统拒绝：`PERMISSION_DENIED`。标准端口普通用户服务未通过**。没有自动修改权限或切换端口 |
| 解压与移动 | ZIP 及独立 CLI 压缩包解压到中文/空格路径后签名保持有效；独立短入口 doctor 和移动后的 `.app` 实际 Ping 1/1 成功 |
| 签名与依赖 | `codesign --verify --deep --strict` 检查 GUI，`--verify --strict` 检查 CLI；Mach-O CPU 与目标匹配；CLI 只依赖 macOS 系统动态库 |

机器可读结果位于 `test-output/integration-release-macos-arm64.json`、`test-output/platform-macos-arm64.json`，发布时只收录与该 CLI SHA-256 相同的结果。macOS 专用脚本中“标准端口”项目验证的是权限观测与错误行为，不将拒绝结果解释为服务可用。Intel 包不附 ARM 运行报告。

## GUI 操作验证

使用实际 `.app` 窗口执行并读取原生可访问性树及截图，未使用浏览器模拟替代 WKWebView。首版已完成以下基础验证；此次包大小修改的专项复测列于下方，当前包内 GUI 报告只记录与当前二进制匹配的专项结果：

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

## 平台差异与待验证项

1. macOS 使用无特权 ICMP 数据报套接字，不运行系统 ping/traceroute，不安装提权 helper。每次收包验证 id、sequence、目标及 Echo 随机载荷，引用错误报文验证原目标和标识；收包每 50ms 检查取消。
2. 标准服务端口在当前用户下不可用。基础评估可手动选择 TFTP 1069 / Syslog 1514；必须使用标准端口时，需要使用者自行在受控环境以管理员身份运行独立 CLI。尚未实现最小特权辅助进程；该限制属于后续 D1/D3 风险，不用高端口通过替代标准端口验收。
3. Intel 实机、macOS 10.15 / 11 最低端、真实设备、VPN/多网卡、链路本地 scope、休眠恢复、两小时压力/轮转与灾难恢复、真实网络下载 quarantine 首次放行均未完成。交叉编译与 Mach-O deployment target 不是运行兼容证明。
4. 未将 CLI 通过推导成全部 GUI 互通通过，未将本机协议对端通过推导成特定路由器/交换机固件认证。未在当前 Mac 复跑 Windows 运行测试；Windows ICMP 实现保留原平台条件分支。
5. TFTP 仅 octet / 512 字节 / 单活动传输，最大 33,553,919 字节；Syslog 仅 UDP / JSONL。AI/MCP、Syslog TCP、索引恢复、自动更新和更新签名等保持原基础版缺项，详见 `README-macOS.md`。
