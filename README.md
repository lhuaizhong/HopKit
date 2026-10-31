# HopKit Network Toolkit — Windows / macOS Preview

[简体中文](README.zh-CN.md)

Version: `0.1.0-alpha.1`. This source tree provides a desktop GUI and standalone CLI for network diagnostics and temporary NTP/FTP/TFTP/Syslog services. See [README-macOS.md](README-macOS.md) for macOS startup, build, architecture, and validation details. The Windows instructions are below.

## Historical downloads

**This branch is HopKit (formerly netbox). No HopKit release is published yet. The following downloads are historical netbox builds and do not include `hk`; build this branch for the new names.**

[Historical release v0.1.0-alpha.1](https://github.com/lhuaizhong/HopKit/releases/tag/v0.1.0-alpha.1) · [SHA256SUMS.txt](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/SHA256SUMS.txt)

| Platform / format | Download |
| --- | --- |
| Windows x64 | [netbox-0.1.0-alpha.1-windows-x64-setup.exe](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-setup.exe) |
| Windows x64 portable EXE | [netbox-0.1.0-alpha.1-windows-x64-portable.exe](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.exe) |
| Windows x64 portable ZIP + CLI | [netbox-0.1.0-alpha.1-windows-x64-portable.zip](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.zip) |
| macOS Apple Silicon DMG | [netbox-0.1.0-alpha.1-macos-arm64.dmg](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-arm64.dmg) |
| macOS Intel DMG | [netbox-0.1.0-alpha.1-macos-x86_64.dmg](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-x86_64.dmg) |

The Release page also includes macOS ZIPs with the CLI and per-file SHA-256 checksums. The Windows installer includes the GUI and CLI and downloads WebView2 if missing. Run the portable EXE directly; the ZIP also includes the CLI. Open the macOS DMG and drag HopKit to Applications. This is a prerelease: Windows binaries are unsigned; macOS apps are ad-hoc signed without Apple notarization, so the OS may warn about an unverified publisher. Both macOS architectures target macOS 11.0 or later; the minimum OS has not been tested on hardware.

## Getting started

Extract the release archive into a writable local directory and run `HopKit.exe`. The GUI requires the system Microsoft Edge WebView2 Runtime, which is usually installed on recent Windows systems. If the GUI cannot start, use `hopkit-cli.exe` independently and install the runtime from [Microsoft's WebView2 page](https://developer.microsoft.com/microsoft-edge/webview2/).

The package does not modify PATH, install system services, add firewall rules, or contact update servers automatically. Basic tools run as an ordinary user. To let devices access temporary listeners, allow inbound traffic according to your local firewall policy.

## Features

| Tool | GUI / CLI capabilities |
| --- | --- |
| Ping | Windows ICMP API / macOS Darwin ICMP; IPv4/IPv6, finite or continuous probes, payload size, timeout, interval, latency, and packet loss |
| Traceroute | ICMP hop-by-hop tracing with three probes per hop and a configurable hop limit; an unresponsive hop alone does not prove a broken route |
| TCPing | TCP connect timing to a specified port, distinguishing successful connections, refusals, and timeouts |
| DNS | System resolver or a specified DNS server; A/AAAA/CNAME/MX/TXT/NS/PTR/SOA queries, with TCP fallback for truncated UDP replies |
| NTP | Client: 1–20 samples, offset/delay/stratum and KoD; temporary NTPv3/v4 UDP server with IPv4/IPv6, source CIDRs and statistics; never changes the system clock |
| iperf3 | Native Rust TCP/UDP throughput client/server; IPv4/IPv6, upload/reverse download, 1–16 TCP streams, single-stream paced UDP, live bitrate, jitter/loss and source CIDRs; no external iperf3 installation |
| FTP | Passive binary client: directory listing, upload and download; temporary IPv4/IPv6 server, anonymous or password login, source CIDRs, read-only by default |
| TFTP | octet/512-byte transfers, upload/download, and a temporary server; atomic local writes, no overwrite by default, read-only server by default |
| Syslog | UDP send/receive, priority extraction, sampled live preview, JSONL files with original message bytes, and segment rotation |

The GUI starts in **English** on first use. Use the **中文 / English** button in the header to switch languages; a saved language preference takes precedence on subsequent launches. The CLI also defaults to English; use `--lang zh-CN` for Chinese result headings. Some preview CLI help and error messages are still bilingual, and machine-readable field names remain English.

The footer’s **GitHub · Share & updates** link opens this project in your default browser for sharing and updates.

The GUI includes appearance switching, live results, a session task center, individual/all-task cancellation, copy, and JSON export. It supports up to four active tasks and retains the latest 40 tasks. Closing the GUI stops its tasks. CLI processes run independently.

Use **Appearance → Fresh / Easy to read** in the header to change styles. Easy to read uses larger text, darker secondary text and status colors, clearer input borders, and stronger keyboard focus indicators. Configuration and results stack vertically in narrower windows. Changes take effect immediately and are saved locally. Fresh is the initial appearance.

Ping accepts payload sizes from 0 to 65,500 bytes, with a default of 32. This is the ICMP data size, excluding IP and ICMP headers. Use `--size` in the CLI; results and exported JSON record it as `payload_size_bytes`. Large-packet reachability depends on MTU, fragmentation, and device policy.

## CLI examples

After a task finishes, use **Export image** or **Copy image** in Live results to save a PNG or copy it to the clipboard. The image preserves the displayed metric cards, charts, table, status, language and appearance. Scrollable result rows are expanded in a detached copy; current filters are respected. JSON, action buttons and the task center are excluded. The live page and its scroll position stay unchanged. Only the retained GUI preview is captured; this is not a complete Syslog log.

Run these commands in PowerShell from the release directory:

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

LAN addresses are examples: replace them with the actual target or local interface address. FTP defaults to TCP 21, NTP to UDP 123, TFTP to UDP 69 and Syslog to UDP 514. Use `--port 2121`, `--port 1123`, `--port 1069` or `--port 1514` to test alternate ports.

GUI and CLI listeners bind to `0.0.0.0` (all IPv4 interfaces) by default. An empty source allowlist accepts all sources. Use `--allow` with comma-separated CIDRs to restrict access. Set `--listen` to a local interface IP, `::` for all IPv6 interfaces, or `127.0.0.1` / `::1` for local-only access.

In the GUI, select **NTP → NTP Server**, set the listen address/port, source CIDRs, stratum and duration, then click **Start service**. A duration of 0 runs until stopped. Live counters show received/replied requests, denied sources and invalid packets; send failures are shown below the preview. Stop, Stop all and quitting the GUI release its listener. Client measurements remain available while the server runs.

The temporary server uses the host system clock as a local reference, with an advertised stratum of 10 by default (`--stratum 1..15`). It does not verify upstream synchronization or adjust the clock; its accuracy depends on the host. It supports only basic 48-byte, unauthenticated NTPv3/v4 client requests and server replies, following the timestamp layout in [RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html#section-8). Authentication/NTS, extension fields, broadcast, peer and control modes are unsupported. Request previews are sampled; counters include every received datagram, with a final `stopped` event on cancellation or duration expiry. These source changes do not alter the previously published release binaries.

### iperf3 network speed test

Select **iperf3** immediately above **TFTP**. Start **Server** on one computer and **Client** on another, using the server IP and port (default 5201). Both peers can use HopKit, or interoperate with standard iperf3. The wire protocol and data transport are implemented in Rust; no executable, libiperf or runtime download is needed.

```sh
hk iperf3 serve --listen 0.0.0.0 --port 5201
hk iperf3 client 192.168.1.10 --duration 10 -P 4
hk iperf3 client 192.168.1.10 --duration 10 -R
hk iperf3 client 192.168.1.10 -u --bitrate-mbps 10 --format json
```

TCP supports 1–16 streams and an optional target rate per stream (0 = unlimited). UDP supports one stream, 1–10000 Mbps and 1200-byte client datagrams. UDP tests also require TCP on the same port. Test duration is 1–3600 seconds; server duration is 0–86400 seconds (0 = until stopped). Servers process one test at a time and support source CIDRs. Live local rates update every 0.5 seconds. Cancellation preserves partial byte counts, elapsed time and local statistics; peer metrics remain unavailable until result exchange. Results support history, rerun, JSON and image export.

Unsupported: iperf2, authentication, SCTP, simultaneous bidirectional tests, omit/warm-up, byte/block-count termination, parallel UDP, GSO/GRO, and custom MSS/window/TOS/congestion tuning. Unsupported client modes are explicitly rejected without stopping the server. TCP retransmits and CPU utilization are not measured; CPU fields are wire-format placeholders, not measurements. Optional standard iperf3 paths in `scripts/iperf3-test.py` are development-only test peers; native tests run with an empty PATH.

Select **FTP** immediately below **TFTP** in the GUI for **List / Download / Upload / Server**. Choose the local root, listen address, port, credentials, source CIDRs and duration for a server. Anonymous read-only access is the default; enable uploads explicitly to create new files. A named account requires a password. Server counters, task cancellation and JSON export use the same task center as the other tools. Passwords are omitted from serialized requests and results. CLI passwords come from `HOPKIT_FTP_PASSWORD` (falling back to `NETBOX_FTP_PASSWORD` only when absent), or the variable named by `--password-env`; for example, set `$env:HOPKIT_FTP_PASSWORD='your-password'` in PowerShell before using `--user operator`. Do not put a password in a target URL.

`shortcuts/hk.exe` is a copy of `hopkit-cli.exe`, with identical arguments, output and exit codes. Use `.\shortcuts\hk.exe --help` without changing PATH. Check for an existing `hk` before adding the shortcuts directory yourself; installers never change PATH or overwrite a system command. New packages do not distribute `nb`.

See [migration and compatibility](MIGRATION.md) for legacy settings, FTP password priority, log directories and package contents. New Windows installers and portable ZIPs contain both CLI entries; the single portable EXE contains only the GUI. macOS ZIPs contain both CLI entries; release DMGs include them in `HopKit.app/Contents/Resources/`.

`--format json` emits one object for a finite task. `--format jsonl` emits events line by line for continuous probes and services. For services, `--duration <seconds>` enables automatic stopping; the default of 0 runs until Ctrl+C. A duration-limited service can also emit a final JSON object.

Exit codes: 0 = diagnostic criterion met; 1 = network/remote result did not meet the criterion; 2 = invalid arguments; 3 = local permissions, resource conflict, or unsupported capability; 4 = local I/O or output failure; 130 = cancelled. Ping/TCPing succeed if at least one probe receives a response; `--strict` requires zero loss. Task `status=succeeded` means the operation completed; inspect `data.success` and the exit code for the diagnostic outcome.

## Preview limitations

- The Windows package targets x64. See `README-macOS.md` for macOS packages and supported execution scope. Verified systems and artifact hashes are recorded in `测试报告.md` and `validation/`. Older LTSC releases, minimum Windows 10 configurations, other architectures, and real device models have not been certified.
- The GUI is portable and uses the installed WebView2 Runtime; no private Fixed Version runtime is bundled. The CLI requires no WebView2, Node, Python, or development tools.
- FTP is plaintext, following basic [RFC 959](https://www.rfc-editor.org/rfc/rfc959.html) commands and [RFC 2428](https://www.rfc-editor.org/rfc/rfc2428.html) EPSV, with IPv4 PASV fallback. There is no FTPS/SFTP, active mode, resume, ASCII file conversion, deletion, rename or directory creation. The server permits four concurrent control sessions, supports LIST/NLST/MLSD and existing subdirectories, and hides unsafe filenames and symbolic links. Select a trusted local root without concurrent external path changes. Uploads create new files atomically and never replace server files; local downloads replace files only with `--overwrite` and after a successful final reply. Other FTP servers may replace their files on upload. Listings are limited to 10,000 entries / 4 MiB. Data ports are dynamically assigned by the OS; LAN firewall rules must allow the application's TCP data connections. After an interrupted upload, check the remote file before retrying. Published release binaries do not include these new source changes.
- TFTP supports octet mode, 512-byte blocks, one active transfer, and single-component filenames within the selected root. There is no blksize negotiation, block-number rollover, netascii, or resume support. The maximum file size is 33,553,919 bytes. Server uploads only create new files and never overwrite existing files. Transfer ports are allocated by the OS rather than a fixed TID range; LAN firewall rules must permit the application's actual UDP traffic.
- TFTP waits one second per block and retries up to five times. The receiver waits two seconds after committing a transfer. If an upload's final ACK is lost, the remote file may already exist. Choose a trusted local directory for the server root.
- Syslog uses UDP and a dedicated receive/write thread, without the planned asynchronous queue, SQLite index, or advanced queries. Production throughput is not guaranteed. After the first ten messages, the preview is sampled at approximately 20 messages per second. The GUI filters the latest 200 preview events; full received messages are saved under `hopkit-syslog/` in the selected directory.
- Syslog preserves original bytes as `raw_hex` alongside decoded text with UTF-8 replacement. Segments are 64 MiB, with a seven-day / approximately 2 GiB retention cap. Cleanup occurs when a segment closes, allowing a temporary overrun of up to one segment. Writes are synchronized every second and again on normal shutdown. Strict crash recovery and index rebuilding are not guaranteed; forced termination can leave a partial final line. Writing stops below 1 GiB of free disk space.
- ICMP and TCP connect calls may wait for the configured timeout, one second by default. Cancellation takes effect when the current call returns. DNS TCP fallback can also wait for read/write timeouts. A cancellation P95 of at most one second is not guaranteed for all settings.
- AI/MCP, automatic updates, update signature verification, permanent history, configuration import/export, and Syslog TCP/TLS are outside this evaluation build. Telemetry is disabled. Diagnostic network requests are initiated by user actions.
- Executables have no commercial code signature and use a local preview application identity. This preview does not represent full acceptance against the v0.3 production design.

## Suggested evaluation sequence

1. Ping `127.0.0.1`, then Ping/Traceroute your gateway. Check success, timeout, cancellation, and English/Chinese display.
2. Run TCPing against known open and closed ports to check connection, refusal, and timeout results.
3. Compare system DNS and corporate DNS A/AAAA results. Check clock offset against a reachable NTP server.
4. Transfer a small TFTP configuration file in a controlled directory and verify its contents before testing real devices. The server is read-only by default; explicitly allow file creation for backup uploads.
5. Start Syslog on UDP 1514 and send a test message to the local LAN IP. Verify both preview and JSONL output, then restrict source networks as needed.
6. Run multiple tasks and check stop-all, GUI shutdown, and independent CLI lifecycles.

When reporting an issue, include the app version, Windows version, tool parameters, expected/actual result, and exported JSON. Review and redact sensitive logs before sharing.

## Development and verification

Use Node 24 (`.nvmrc`: 24.14.0) and the pinned Rust 1.96.0 toolchain. The historical Windows package used Rust 1.94.0, Node 24.3.0, and Visual Studio 2022 Build Tools. Dependencies are locked by `Cargo.lock` and `package-lock.json`. The original Windows package uses Rust 1.94.0; `rust-toolchain.toml` currently pins 1.96.0 for the macOS extension. GitHub release builds use the pinned toolchain and run core tests, local protocol integration tests, and installer/GUI launch smoke checks. Keep the dependency lockfiles intact.

```powershell
# Node 24 (see .nvmrc); select Node 24 before installing.
npm ci
npm run build
# Appearance checks require development-only puppeteer-core and a browser:
# $env:HOPKIT_BROWSER='absolute path to your Chrome/Chromium executable'
# node scripts/appearance-test.mjs
cargo test -p hopkit-core
cargo build --release --features hopkit-desktop/custom-protocol -p hopkit-desktop -p hopkit-cli
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
powershell -NoProfile -File scripts/package.ps1
```

For GUI development, run `npm run desktop`. The integration script uses only Python's standard library and independent local protocol peers on temporary ports, without public Internet access or real devices. Reports contain per-scenario evidence. GUI CDP tests enable the WebView2 debugging port only for test launches; normal release startup does not enable it.

Release packaging includes both `README.md` and `README.zh-CN.md`; Windows also keeps `使用说明.md` as a Chinese-language convenience copy. Build-time defaults apply to new installations; rebuilding or upgrading does not discard an existing GUI language preference.

## Source and feedback

The public source repository is [lhuaizhong/HopKit](https://github.com/lhuaizhong/HopKit). Clone it with:

```sh
git clone https://github.com/lhuaizhong/HopKit.git
cd HopKit
```

Follow the development instructions above for Windows or [README-macOS.md](README-macOS.md) for macOS. Report bugs and feature requests through [GitHub Issues](https://github.com/lhuaizhong/HopKit/issues), with sensitive device addresses, configuration files, and log contents removed. Build outputs and local test artifacts are excluded from Git history.

## License

Project source code is available under the [MIT License](LICENSE), copyright 2026 Huaizhong Liu. Third-party dependencies retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the notices included in release packages.

### Diagnostic statistics and tasks

Ping/TCPing use backend cumulative counters independently of the bounded recent preview. Stopped tasks retain partial results; stopping before the first measurement explicitly shows no samples. Attempted calls, completed measurements and interrupted calls are separate. Latency and failure rates exclude interrupted calls. TCPing labels its measurements as TCP connection time and connection failures.

Traceroute has dedicated hop count, target reached, elapsed time and timeout-probe metrics, with three individual probes and response addresses per hop. TTL expiration is a hop response, not destination success; intermediate timeouts do not prove a broken link. Trace JSON now uses `schema_version: 2` and omits cross-hop Ping loss metrics. Other commands retain version 1, and cancellation keeps exit code 130. See [DIAGNOSTIC-PROTOCOL.md](DIAGNOSTIC-PROTOCOL.md).

The chart has a linear ms axis, time axis, failure markers and time/value/status tooltips. Filter the available recent samples and choose a 20/80/200-sample window. PNG captures that view; cumulative metrics always cover the entire task.

Tool and operation configurations are separate within the session. Transfer the last task's target explicitly. Opening a task preserves the form; rerun loads its original request for review before starting. FTP passwords are excluded from configuration caches, task snapshots and exports, and must be re-entered. The task center exposes up to 40 session tasks, with active tasks pinned above filterable history and individually stoppable. Persistence across application restarts remains a later-stage feature.

### Endpoint troubleshooting workflow (stage 3, source addition)

The desktop opens **Troubleshooting** by default: review local interfaces, addresses/prefixes, gateways, DNS and default routes; enter a target; then inspect staged evidence, possible causes and next verification steps. Local state refreshes every 15 seconds without public probes. Advanced options are collapsed, and individual tools remain available.

```sh
hk network --format json
hk target 'https://example.com:8443/path?token=example' --format json
hk check example.com --format json
hk check 192.168.1.1:443 --format json
hk check 'https://example.com/path' --source 192.168.1.10 --format json
hk check example.com --dns-server 192.168.1.1 --trace --format jsonl
```

Accepts IPs, IDNs, host:port, `[IPv6]:port` and HTTP(S) URLs, with a normalized preview. A bare host does not imply an HTTP service. URL credentials are rejected. Original query parameters are used for the request but queries/fragments are removed from snapshots, JSON, PNG and rerun parameters; re-enter the original URL before rerunning a redacted target. Explicit handoff actions preserve the host and relevant port without passing URL paths to Ping.

TCP/HTTP actually bind the selected source IP and record local/remote addresses. System DNS, direct DNS comparisons and route queries follow system policy without source binding. Source-bound ICMP/Trace, named IPv6 scopes and URL zone identifiers are unsupported and explicitly skipped/rejected. Use numeric interface scopes for link-local TCP endpoints. Selecting a source IP does not pin a physical egress interface through VPN/routing changes.

HTTP/TLS uses compiled-in Rust HTTP/1.1, rustls and bundled Mozilla roots. It does not invoke curl, OpenSSL or other third-party programs, or read PATH, curl configuration, proxy or certificate environment overrides. Certificate validation is enabled by default. Explicitly add an enterprise PEM CA with advanced options or `--ca-file /path/to/ca.pem` (up to 1 MiB). The request snapshot records its path, without exporting certificate file contents. `--insecure-tls` visibly marks chain, name and time validation as skipped.

HEAD reads no body. Response headers are limited to 64 KiB, 128 fields and 8 interim responses, with at most 5 redirects. Credentials, non-HTTP(S) redirects and HTTPS downgrades are rejected; no cookies, Authorization or Referer are sent. HEAD and GET may differ. This version uses HTTP/1.1 without system proxies or system certificate stores. Bundled roots update with releases; online revocation checks and fetching missing intermediates are not implemented.

Execution completion is separate from diagnostic observations: HTTP 403/500, certificate errors and connection refusal retain their actual stages. Stop/Ctrl+C preserves partial evidence and exits 130. Timestamps use UTC; stage elapsed includes adapter overhead while protocol timing is separate. System DNS has a 5-second bound; each standalone TCP attempt and each entire HTTP hop (connect + TLS + response) uses the requested timeout, up to 8 candidate addresses; optional Trace has a 120-second bound. A single session does not establish long-term health or fault ownership.

See [stage 3 verification and limitations](测试报告-第三阶段终端排障.md). Historical download assets do not include these source changes.
