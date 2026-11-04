# Changelog / 版本记录

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
