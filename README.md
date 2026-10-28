# netbox Network Toolkit — Windows / macOS Preview

[简体中文](README.zh-CN.md)

Version: `0.1.0-alpha.1`. This preview provides a desktop GUI and standalone CLI for network diagnostics and temporary TFTP/Syslog services. See [README-macOS.md](README-macOS.md) for macOS startup, build, architecture, and validation details. The Windows instructions are below.

## Getting started

Extract the release archive into a writable local directory and run `netbox.exe`. The GUI requires the system Microsoft Edge WebView2 Runtime, which is usually installed on recent Windows systems. If the GUI cannot start, use `netbox-cli.exe` independently and install the runtime from [Microsoft's WebView2 page](https://developer.microsoft.com/microsoft-edge/webview2/).

The package does not modify PATH, install system services, add firewall rules, or contact update servers automatically. Basic tools run as an ordinary user. To let devices access temporary listeners, allow inbound traffic according to your local firewall policy.

## Features

| Tool | GUI / CLI capabilities |
| --- | --- |
| Ping | Windows ICMP API / macOS Darwin ICMP; IPv4/IPv6, finite or continuous probes, payload size, timeout, interval, latency, and packet loss |
| Traceroute | ICMP hop-by-hop tracing with three probes per hop and a configurable hop limit; an unresponsive hop alone does not prove a broken route |
| TCPing | TCP connect timing to a specified port, distinguishing successful connections, refusals, and timeouts |
| DNS | System resolver or a specified DNS server; A/AAAA/CNAME/MX/TXT/NS/PTR/SOA queries, with TCP fallback for truncated UDP replies |
| NTP | 1–20 samples, clock offset, delay, stratum, and KoD handling; never changes the system clock |
| TFTP | octet/512-byte transfers, upload/download, and a temporary server; atomic local writes, no overwrite by default, read-only server by default |
| Syslog | UDP send/receive, priority extraction, sampled live preview, JSONL files with original message bytes, and segment rotation |

The GUI starts in **English** on first use. Use the **中文 / English** button in the header to switch languages; a saved language preference takes precedence on subsequent launches. The CLI also defaults to English; use `--lang zh-CN` for Chinese result headings. Some preview CLI help and error messages are still bilingual, and machine-readable field names remain English.

The GUI includes appearance switching, live results, a session task center, individual/all-task cancellation, copy, and JSON export. It supports up to four active tasks and retains the latest 40 tasks. Closing the GUI stops its tasks. CLI processes run independently.

Use **Appearance → Fresh / Easy to read** in the header to change styles. Easy to read uses larger text, darker secondary text and status colors, clearer input borders, and stronger keyboard focus indicators. Configuration and results stack vertically in narrower windows. Changes take effect immediately and are saved locally. Fresh is the initial appearance.

Ping accepts payload sizes from 0 to 65,500 bytes, with a default of 32. This is the ICMP data size, excluding IP and ICMP headers. Use `--size` in the CLI; results and exported JSON record it as `payload_size_bytes`. Large-packet reachability depends on MTU, fragmentation, and device policy.

## CLI examples

After a task finishes, use **Export image** or **Copy image** in Live results to save a PNG or copy it to the clipboard. The image preserves the displayed metric cards, charts, table, status, language and appearance. Scrollable result rows are expanded in a detached copy; current filters are respected. JSON, action buttons and the task center are excluded. The live page and its scroll position stay unchanged. Only the retained GUI preview is captured; this is not a complete Syslog log.

Run these commands in PowerShell from the release directory:

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

LAN addresses are examples: replace them with the actual target or local interface address. TFTP defaults to UDP 69 and Syslog to UDP 514. Use `--port 1069` or `--port 1514` to test alternate ports.

GUI and CLI listeners bind to `0.0.0.0` (all IPv4 interfaces) by default. An empty source allowlist accepts all sources. Use `--allow` with comma-separated CIDRs to restrict access. Set `--listen` to a local interface IP, `::` for all IPv6 interfaces, or `127.0.0.1` / `::1` for local-only access.

`shortcuts/nb.exe` is an optional short entry point for the same version. Invoke it by relative path or add it to your own PATH after checking for an existing `nb` command. The full executable works independently of this shortcut.

`--format json` emits one object for a finite task. `--format jsonl` emits events line by line for continuous probes and services. For services, `--duration <seconds>` enables automatic stopping; the default of 0 runs until Ctrl+C. A duration-limited service can also emit a final JSON object.

Exit codes: 0 = diagnostic criterion met; 1 = network/remote result did not meet the criterion; 2 = invalid arguments; 3 = local permissions, resource conflict, or unsupported capability; 4 = local I/O or output failure; 130 = cancelled. Ping/TCPing succeed if at least one probe receives a response; `--strict` requires zero loss. Task `status=succeeded` means the operation completed; inspect `data.success` and the exit code for the diagnostic outcome.

## Preview limitations

- The Windows package targets x64. See `README-macOS.md` for macOS packages and supported execution scope. Verified systems and artifact hashes are recorded in `测试报告.md` and `validation/`. Older LTSC releases, minimum Windows 10 configurations, other architectures, and real device models have not been certified.
- The GUI is portable and uses the installed WebView2 Runtime; no private Fixed Version runtime is bundled. The CLI requires no WebView2, Node, Python, or development tools.
- TFTP supports octet mode, 512-byte blocks, one active transfer, and single-component filenames within the selected root. There is no blksize negotiation, block-number rollover, netascii, or resume support. The maximum file size is 33,553,919 bytes. Server uploads only create new files and never overwrite existing files. Transfer ports are allocated by the OS rather than a fixed TID range; LAN firewall rules must permit the application's actual UDP traffic.
- TFTP waits one second per block and retries up to five times. The receiver waits two seconds after committing a transfer. If an upload's final ACK is lost, the remote file may already exist. Choose a trusted local directory for the server root.
- Syslog uses UDP and a dedicated receive/write thread, without the planned asynchronous queue, SQLite index, or advanced queries. Production throughput is not guaranteed. After the first ten messages, the preview is sampled at approximately 20 messages per second. The GUI filters the latest 200 preview events; full received messages are saved under `netbox-syslog/` in the selected directory.
- Syslog preserves original bytes as `raw_hex` alongside decoded text with UTF-8 replacement. Segments are 64 MiB, with a seven-day / approximately 2 GiB retention cap. Cleanup occurs when a segment closes, allowing a temporary overrun of up to one segment. Writes are synchronized every second and again on normal shutdown. Strict crash recovery and index rebuilding are not guaranteed; forced termination can leave a partial final line. Writing stops below 1 GiB of free disk space.
- ICMP and TCP connect calls may wait for the configured timeout, one second by default. Cancellation takes effect when the current call returns. DNS TCP fallback can also wait for read/write timeouts. A cancellation P95 of at most one second is not guaranteed for all settings.
- AI/MCP, installers, automatic updates, update signature verification, permanent history, configuration import/export, FTP, and Syslog TCP/TLS are outside this evaluation build. Telemetry is disabled. Diagnostic network requests are initiated by user actions.
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

The verified Windows toolchain uses Rust 1.94.0, Node 24.3.0, and Visual Studio 2022 Build Tools. Dependencies are locked by `Cargo.lock` and `package-lock.json`. The original Windows package uses Rust 1.94.0; `rust-toolchain.toml` currently pins 1.96.0 for the macOS extension. A Windows release regression on that newer toolchain has not been completed. Keep the dependency lockfiles intact.

```powershell
npm ci
npm run build
# Appearance checks require development-only puppeteer-core and a browser:
# $env:NETBOX_BROWSER='absolute path to your Chrome/Chromium executable'
# node scripts/appearance-test.mjs
cargo test -p netbox-core
cargo build --release --features netbox-desktop/custom-protocol -p netbox-desktop -p netbox-cli
python scripts/integration_test.py --exe target/release/netbox-cli.exe
powershell -NoProfile -File scripts/package.ps1
```

For GUI development, run `npm run desktop`. The integration script uses only Python's standard library and independent local protocol peers on temporary ports, without public Internet access or real devices. Reports contain per-scenario evidence. GUI CDP tests enable the WebView2 debugging port only for test launches; normal release startup does not enable it.

Release packaging includes both `README.md` and `README.zh-CN.md`; Windows also keeps `使用说明.md` as a Chinese-language convenience copy. Build-time defaults apply to new installations; rebuilding or upgrading does not discard an existing GUI language preference.

## Source and feedback

The public source repository is [lhuaizhong/netbox](https://github.com/lhuaizhong/netbox). Clone it with:

```sh
git clone https://github.com/lhuaizhong/netbox.git
cd netbox
```

Follow the development instructions above for Windows or [README-macOS.md](README-macOS.md) for macOS. Report bugs and feature requests through [GitHub Issues](https://github.com/lhuaizhong/netbox/issues), with sensitive device addresses, configuration files, and log contents removed. Build outputs and local test artifacts are excluded from Git history.

## License

Project source code is available under the [MIT License](LICENSE), copyright 2026 Huaizhong Liu. Third-party dependencies retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the notices included in release packages.
