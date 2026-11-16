# Changelog / 版本记录

## 0.2.0-dev.5 — M4 / 2026-11-16

- Expand the sidebar tool list into the space previously used by the preview/version/local-first block. Move multi-target Monitor and temporary Syslog from Common to Expert, preserving saved settings and favorites.
- Add shared Rust TCP/UDP debug channels, native task-scoped send/half-close/close controls, binary-safe HEX/UTF-8 previews, bounded scheduled sends, source restrictions and opt-in capped payload recording. GUI and CLI use the same core.
- Add read-only local TCP/UDP endpoint inspection with Windows IP Helper PID/name evidence, explicit partial macOS visibility, filters, non-overlapping refresh and port-conflict handoff. Add IPv4 Wake-on-LAN with exact Magic Packets, source binding and interface-prefix broadcast suggestions.
- 新增 TCP/UDP 调试助手、本机端口与连接查看器及 Wake-on-LAN；正文默认仅在有限内存预览中保存，通用任务导出排除正文。已在 Windows 验证独立对端、原生 IPC 与 UDP 监听两小时稳定性；macOS、真实设备唤醒仍待验收，详见 [M4 记录](docs/M4-开发与验收记录.md)。

## 0.2.0-dev.4 — M3 / 2026-11-15

- Add bounded ARP/ICMP/TCP device discovery for explicit IPv4 ranges and IPv6 candidates, with shared Scan admission/rate budgets, source binding, cancellation and partial results.
- Keep active responses, read-only neighbor cache and optional bounded PTR evidence separate. Preserve link/state/time/MAC sources, surface MAC change clues without asserting conflict, and treat redacted MACs as unknown.
- Add `hk discover`, Expert Discovery GUI, filtering/paging/full JSON export, saved profiles and manual handoff to Scan/Ping/TCPing/Check without execution. Preserve bilingual navigation and both appearances.
- 新增 M3 设备发现、邻居来源、可选 PTR、MAC 变化线索与跨工具手动联动；未响应始终为未知，不判断空闲 IP。发现结果仅显示响应设备并按 IP 排序，MAC 直显；ARP 仅限实际路由直连的 IPv4 目标。macOS BPF 权限、主动 ND、OUI 厂商及三平台现场验收的边界见 [M3 开发与验收记录](docs/M3-开发与验收记录.md)。

## 0.2.0-dev.3 — M2 / 2026-11-14

- Add offline IPv4/IPv6 subnet inspection, containment/overlap, bounded splitting and exact aggregation. Preserve exact decimal counts through IPv6 /0. CLI and Utilities GUI share the same core; calculation takes no network task slot.
- Add single/multi-host TCP connect scanning with fixed DNS candidates, source binding, bounded fair workers, shared connection/rate limits, cancellation and partial results. Include CLI text/JSON/JSONL, Expert GUI presets, pagination, full JSON export and manual TCPing handoff.
- Extend saved profiles, search, favorites and capabilities for both tools; group navigation into Common / File Transfer / Expert / Utilities. TFTP, FTP, HTTP File Sharing and Wormhole move into File Transfer; old navigation follows stable tool IDs with settings and favorites preserved. Synchronize source versions to `0.2.0-dev.3`.
- Arrange category buttons in a compact two-column grid to leave more sidebar height for the tool list, with complete bilingual labels and running-task badges.
- 新增离线 IPv4/IPv6 子网计算、包含/重叠、有限拆分与无空洞聚合，精确保留大整数，计算不占网络任务名额；GUI 使用摘要、范围、直观结论和分页网段列表，原始 JSON 默认折叠。
- 新增单主机/有限多主机 TCP connect 扫描、共享预算与公平限速、固定候选、源绑定、取消及部分结果；提供 CLI、专家工具入口、分页/完整导出与手动 TCPing 联动。
- TCP Scan 启动时源 IP 留空，默认使用系统路由；固定源绑定须手动填写或主动载入现场配置。原生验收窗口使用隔离存储，避免测试参数污染用户配置。
- 导航调整为常用工具 / 文件传输 / 专家工具 / 小工具；TFTP、FTP、HTTP 文件服务和 Wormhole 独立成组，旧导航按工具 ID 迁移，保留参数和收藏。
- 分类按钮改为紧凑的两列两行布局，保留完整中英文名称和运行任务数，为实际工具列表腾出更多高度。
- 两类新工具接入配置保存、查找收藏和能力接口，详情见 [M2 开发与验收记录](docs/M2-开发与验收记录.md)。

## 0.2.0-dev.2 — M1 / 2026-11-14

- Add Common/Expert/Utilities navigation, bilingual global search with exact-name priority, favorites, remembered navigation/parameters and a global active-task entry. All 13 existing GUI tools remain in Common; future categories explain their current empty state.
- Save named targets and site profiles locally. Versioned JSON import/export validates fields, counts and total size; passwords, pairing codes, message bodies and URL queries/fragments are excluded. Loading settings does not start tasks.
- Introduce stable Rust/Vue tool IDs, capability queries and shared probe-budget interfaces. Keep the four-task admission limit, task/event bounds and runtime credential isolation.
- 三分类导航、双语跨分类搜索、工具收藏、导航和参数恢复，以及全局任务入口；已有 13 个工具归常用工具，后续工具验收后再加入其他分类。
- 支持目标收藏、命名现场配置及有校验的 JSON 导入导出；密码、配对码、报文正文和 URL 查询/片段不进入配置库，载入配置不执行任务。
- 增加稳定工具 ID、能力查询和共用预算接口，保留四任务上限与旧工具行为。详情见 [M1 开发与验收记录](docs/M1-开发与验收记录.md)。

## 0.2.0-dev.1 — In development / 开发中

- Start the 0.2.0 development line and synchronize workspace, npm and desktop versions.
- Add the [technical development plan](docs/HopKit-0.2.0-技术开发计划书.md), covering new tools, Common/Expert/Utilities navigation, shared architecture, delivery order and acceptance criteria. Planned features remain pending implementation.
- 开启 0.2.0 功能集开发，统一 Rust、npm 与桌面端版本号。
- 新增技术开发计划书，明确常用工具/专家工具/小工具分类，以及新工具、现有功能扩展、数据契约、平台限制和逐项验收。计划中的功能和导航调整尚待实现。

## 0.1.0-rc.2 — 2026-11-04

- Separate Auto / IPv4 / IPv6 in GUI clients and add exclusive CLI `-4` / `-6` flags. Explicit IPv4 filters dual-stack results without IPv6 fallback.
- GUI 客户端区分自动 / IPv4 / IPv6；CLI 增加互斥的 `-4` / `-6` 参数，双栈目标可明确使用 IPv4，不回退到 IPv6。

- Make the default CLI text output concise: per-probe or per-hop results, essential protocol data, and final summaries. Keep full JSON/JSONL output available explicitly.
- 简化 CLI 默认文本输出：按次或按跳显示结果，仅保留必要协议数据和最终汇总；显式选择 JSON/JSONL 时继续输出完整数据。

## 0.1.0-rc.1 — 2026-11-02

- Add HTTP file sharing directly below FTP in the desktop sidebar, with default
  port 80, configurable port/directory, optional uploads, browser URLs and counters.
- Include a browser page with subdirectory navigation, multiple uploads, progress
  and cancellation, downloads and English/Chinese labels. Files stream to disk;
  uploads never overwrite and incomplete transfers are cleaned up.
- Add `hk http serve --root <directory> [--port 8080] [--write]`.

新增 HTTP 文件服务：入口位于 FTP 下一行，默认端口 80，可修改。浏览器可直接
浏览目录和下载文件，勾选允许上传后支持多文件上传、进度和取消，无需额外软件。
同名文件不覆盖，未完成上传会清理；支持中英文界面及 IPv4/IPv6。

## 0.1.0-beta.1 — 2026-11-01

**HopKit Pre-release 1 / HopKit 首个预发布版本**

The first release under the HopKit name includes the current GUI and independent
Rust CLI. The former netbox `v0.1.0-alpha.1` remains available as a historical release.

- Integrated troubleshooting: local network overview, normalized targets, staged DNS,
  routes, ICMP, TCP and HTTP/TLS evidence, possible causes and next steps.
- Multi-target monitoring: up to three ICMP/TCP targets on one timeline, failure and
  recovery periods, pause/resume, notes, network events and before/after comparison.
- Ping, Traceroute, TCPing, DNS, NTP client/server, built-in iperf3 TCP/UDP,
  FTP/TFTP client/server and UDP Syslog collection.
- English/Chinese interface, Fresh/Easy to read appearance, task center, JSON and PNG
  exports, and the `hk` CLI entry.
- Native Windows x64 installer and portable EXE/ZIP; Apple Silicon and Intel DMGs,
  application ZIPs and standalone CLI ZIPs. Packages include dependency notices,
  build metadata and SHA-256 checksums.
- Preserve the iperf3 source-denial response on Windows instead of losing it to a
  TCP reset while closing an unread handshake.

首个以 HopKit 名称发布的版本，包含当前 GUI 与独立 Rust CLI：

- 集成网络排障首页，关联本机网络、DNS、路由、ICMP、TCP 和 HTTP/TLS 证据。
- 最多三个 ICMP/TCP 目标共用监测时间轴，支持故障恢复区段、暂停恢复、备注、
  网络事件及前后范围对比。
- 完整基础网络工具与临时服务，内置 iperf3，无需另装测速程序。
- 中英双语、两种界面风格、任务中心、JSON/PNG 导出及 `hk` 短入口。
- Windows x64 安装版与免安装版、macOS 两种 CPU 的 DMG/ZIP 和独立 CLI ZIP。

This is a testing release. Windows binaries are unsigned; macOS uses ad-hoc signing
without Apple notarization. Native release checks cover core tests, local protocol
peers, a 35-second monitoring fault/recovery run, and packaged CLI entries. These do
not certify minimum OS versions, extended endurance, physical sleep/VPN/interface
changes, or every native GUI workflow. See [monitoring boundaries](MONITORING.md).

此版本用于测试与评估。Windows 未做代码签名，macOS 未做 Apple 公证。发布检查不等同于
最低系统版本、长时间运行、物理休眠/网卡/VPN 切换或完整 GUI 流程验收。

## 0.1.0-alpha.1 — 2026-10-28

Historical netbox preview. Asset names retain `netbox`; later HopKit capabilities
and the `hk` entry are available in `0.1.0-beta.1`.

## 0.1.0-rc.2 — Wormhole

- Added shared Rust File Transfer v1 core, GUI send/receive and CLI commands with one-time codes, confirmation, streaming SHA-256, cancellation and no-overwrite publication.
- Added bounded protocol/DNS patches to vendored magic-wormhole 0.8.1; sensitive code isolation and EUPL corresponding-source packaging.
