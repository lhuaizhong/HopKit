# netbox Network Toolkit — Windows / macOS Preview

[简体中文](README.zh-CN.md)

Version: `0.1.0-alpha.1`. This source tree provides a desktop GUI and standalone CLI for network diagnostics and temporary NTP/FTP/TFTP/Syslog services. See [README-macOS.md](README-macOS.md) for macOS startup, build, architecture, and validation details. The Windows instructions are below.

## Downloads

[Release v0.1.0-alpha.1](https://github.com/lhuaizhong/netbox/releases/tag/v0.1.0-alpha.1) · [SHA256SUMS.txt](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/SHA256SUMS.txt)

| Platform / format | Download |
| --- | --- |
| Windows x64 | [netbox-0.1.0-alpha.1-windows-x64-setup.exe](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-setup.exe) |
| Windows x64 portable EXE | [netbox-0.1.0-alpha.1-windows-x64-portable.exe](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.exe) |
| Windows x64 portable ZIP + CLI | [netbox-0.1.0-alpha.1-windows-x64-portable.zip](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.zip) |
| macOS Apple Silicon DMG | [netbox-0.1.0-alpha.1-macos-arm64.dmg](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-arm64.dmg) |
| macOS Intel DMG | [netbox-0.1.0-alpha.1-macos-x86_64.dmg](https://github.com/lhuaizhong/netbox/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-x86_64.dmg) |

The Release page also includes macOS ZIPs with the CLI and per-file SHA-256 checksums. The Windows installer includes the GUI and CLI and downloads WebView2 if missing. Run the portable EXE directly; the ZIP also includes the CLI. Open the macOS DMG and drag netbox to Applications. This is a prerelease: Windows binaries are unsigned; macOS apps are ad-hoc signed without Apple notarization, so the OS may warn about an unverified publisher. Both macOS architectures target macOS 11.0 or later; the minimum OS has not been tested on hardware.

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
| NTP | Client: 1–20 samples, offset/delay/stratum and KoD; temporary NTPv3/v4 UDP server with IPv4/IPv6, source CIDRs and statistics; never changes the system clock |
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
.\netbox-cli.exe ping 127.0.0.1
.\netbox-cli.exe ping 127.0.0.1 --size 1472
.\netbox-cli.exe ping ::1 -6 -c 4 --format json
.\netbox-cli.exe trace 192.168.1.1 --max-hops 15
.\netbox-cli.exe tcping 192.168.1.1 443 -c 10
.\netbox-cli.exe dns example.com
.\netbox-cli.exe dns example.com AAAA -s 192.168.1.1
.\netbox-cli.exe ntp 192.168.1.10 -c 4
.\netbox-cli.exe ntp serve --listen 192.168.1.100 --allow 192.168.1.0/24
.\netbox-cli.exe ntp serve --listen 127.0.0.1 --port 1123 --duration 60 --format jsonl
.\netbox-cli.exe ftp list 192.168.1.10 /backups --user operator
.\netbox-cli.exe ftp get 192.168.1.10 /backups/config.cfg -o .\config.cfg
.\netbox-cli.exe ftp put 192.168.1.10 .\config.cfg --remote-name /backups/config.cfg
.\netbox-cli.exe ftp serve --root D:\FtpFiles --port 2121 --allow 192.168.1.0/24 --write
.\netbox-cli.exe tftp get 192.168.1.10 config.cfg -o .\config.cfg
.\netbox-cli.exe tftp put 192.168.1.10 .\config.cfg
.\netbox-cli.exe tftp serve --root D:\TftpFiles --listen 192.168.1.100 --allow 192.168.1.0/24
.\netbox-cli.exe syslog listen --listen 192.168.1.100 --allow 192.168.1.0/24 --dir D:\DeviceLogs
.\netbox-cli.exe syslog send 192.168.1.10 "netbox test"
.\netbox-cli.exe doctor --format json
.\netbox-cli.exe --help
```

LAN addresses are examples: replace them with the actual target or local interface address. FTP defaults to TCP 21, NTP to UDP 123, TFTP to UDP 69 and Syslog to UDP 514. Use `--port 2121`, `--port 1123`, `--port 1069` or `--port 1514` to test alternate ports.

GUI and CLI listeners bind to `0.0.0.0` (all IPv4 interfaces) by default. An empty source allowlist accepts all sources. Use `--allow` with comma-separated CIDRs to restrict access. Set `--listen` to a local interface IP, `::` for all IPv6 interfaces, or `127.0.0.1` / `::1` for local-only access.

In the GUI, select **NTP → NTP Server**, set the listen address/port, source CIDRs, stratum and duration, then click **Start service**. A duration of 0 runs until stopped. Live counters show received/replied requests, denied sources and invalid packets; send failures are shown below the preview. Stop, Stop all and quitting the GUI release its listener. Client measurements remain available while the server runs.

The temporary server uses the host system clock as a local reference, with an advertised stratum of 10 by default (`--stratum 1..15`). It does not verify upstream synchronization or adjust the clock; its accuracy depends on the host. It supports only basic 48-byte, unauthenticated NTPv3/v4 client requests and server replies, following the timestamp layout in [RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html#section-8). Authentication/NTS, extension fields, broadcast, peer and control modes are unsupported. Request previews are sampled; counters include every received datagram, with a final `stopped` event on cancellation or duration expiry. These source changes do not alter the previously published release binaries.

Select **FTP** immediately below **TFTP** in the GUI for **List / Download / Upload / Server**. Choose the local root, listen address, port, credentials, source CIDRs and duration for a server. Anonymous read-only access is the default; enable uploads explicitly to create new files. A named account requires a password. Server counters, task cancellation and JSON export use the same task center as the other tools. Passwords are omitted from serialized requests and results. CLI passwords come from `NETBOX_FTP_PASSWORD`, or the variable named by `--password-env`; for example, set `$env:NETBOX_FTP_PASSWORD='your-password'` in PowerShell before using `--user operator`. Do not put a password in a target URL.

`shortcuts/nb.exe` is an optional short entry point for the same version. Invoke it by relative path or add it to your own PATH after checking for an existing `nb` command. The full executable works independently of this shortcut.

`--format json` emits one object for a finite task. `--format jsonl` emits events line by line for continuous probes and services. For services, `--duration <seconds>` enables automatic stopping; the default of 0 runs until Ctrl+C. A duration-limited service can also emit a final JSON object.

Exit codes: 0 = diagnostic criterion met; 1 = network/remote result did not meet the criterion; 2 = invalid arguments; 3 = local permissions, resource conflict, or unsupported capability; 4 = local I/O or output failure; 130 = cancelled. Ping/TCPing succeed if at least one probe receives a response; `--strict` requires zero loss. Task `status=succeeded` means the operation completed; inspect `data.success` and the exit code for the diagnostic outcome.

## Preview limitations

- The Windows package targets x64. See `README-macOS.md` for macOS packages and supported execution scope. Verified systems and artifact hashes are recorded in `测试报告.md` and `validation/`. Older LTSC releases, minimum Windows 10 configurations, other architectures, and real device models have not been certified.
- The GUI is portable and uses the installed WebView2 Runtime; no private Fixed Version runtime is bundled. The CLI requires no WebView2, Node, Python, or development tools.
- FTP is plaintext, following basic [RFC 959](https://www.rfc-editor.org/rfc/rfc959.html) commands and [RFC 2428](https://www.rfc-editor.org/rfc/rfc2428.html) EPSV, with IPv4 PASV fallback. There is no FTPS/SFTP, active mode, resume, ASCII file conversion, deletion, rename or directory creation. The server permits four concurrent control sessions, supports LIST/NLST/MLSD and existing subdirectories, and hides unsafe filenames and symbolic links. Select a trusted local root without concurrent external path changes. Uploads create new files atomically and never replace server files; local downloads replace files only with `--overwrite` and after a successful final reply. Other FTP servers may replace their files on upload. Listings are limited to 10,000 entries / 4 MiB. Data ports are dynamically assigned by the OS; LAN firewall rules must allow the application's TCP data connections. After an interrupted upload, check the remote file before retrying. Published release binaries do not include these new source changes.
- TFTP supports octet mode, 512-byte blocks, one active transfer, and single-component filenames within the selected root. There is no blksize negotiation, block-number rollover, netascii, or resume support. The maximum file size is 33,553,919 bytes. Server uploads only create new files and never overwrite existing files. Transfer ports are allocated by the OS rather than a fixed TID range; LAN firewall rules must permit the application's actual UDP traffic.
- TFTP waits one second per block and retries up to five times. The receiver waits two seconds after committing a transfer. If an upload's final ACK is lost, the remote file may already exist. Choose a trusted local directory for the server root.
- Syslog uses UDP and a dedicated receive/write thread, without the planned asynchronous queue, SQLite index, or advanced queries. Production throughput is not guaranteed. After the first ten messages, the preview is sampled at approximately 20 messages per second. The GUI filters the latest 200 preview events; full received messages are saved under `netbox-syslog/` in the selected directory.
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

The verified Windows toolchain uses Rust 1.94.0, Node 24.3.0, and Visual Studio 2022 Build Tools. Dependencies are locked by `Cargo.lock` and `package-lock.json`. The original Windows package uses Rust 1.94.0; `rust-toolchain.toml` currently pins 1.96.0 for the macOS extension. GitHub release builds use the pinned toolchain and run core tests, local protocol integration tests, and installer/GUI launch smoke checks. Keep the dependency lockfiles intact.

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
