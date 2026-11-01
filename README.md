# HopKit

**Network diagnostics and temporary services for Windows and macOS**

[简体中文](README.zh-CN.md) · [macOS guide](README-macOS.md) · [Report an issue](https://github.com/lhuaizhong/HopKit/issues)

HopKit combines endpoint troubleshooting, network measurements, file transfer, and log collection in a desktop GUI and an independent CLI. Both share a Rust core, providing consistent results for interactive work and automation.

**Version: `0.1.0-alpha.1` — evaluation preview.** This README describes the current source tree. Historical downloads use the former **netbox** name and do not include `hk` or subsequent source additions. Build from source to use the capabilities documented here.

## Capabilities

| Tool | Capabilities |
| --- | --- |
| Troubleshooting | Local network overview; staged DNS, route, ICMP, TCP, and HTTP/TLS evidence; possible causes and next steps |
| Network / target | Inspect interfaces, addresses, gateways, DNS, and default routes; normalize targets without sending probes |
| Ping | IPv4/IPv6, finite or continuous probes, configurable payload/timeout/interval, latency, and packet loss |
| Traceroute | ICMP tracing, three probes per hop, configurable hop limit, response addresses, and destination status |
| TCPing | TCP connection timing with separate success, refusal, and timeout results |
| DNS | System resolver or a specified server; A/AAAA/CNAME/MX/TXT/NS/PTR/SOA queries; TCP fallback for truncated UDP replies |
| NTP | Clock offset, delay, stratum, and KoD detection; temporary NTPv3/v4 server; no system clock changes |
| iperf3 | Built-in TCP/UDP throughput client/server, reverse download, parallel TCP streams, UDP jitter/loss, and live rates |
| FTP | Passive client with directory listing and binary upload/download; temporary IPv4/IPv6 server with anonymous or password login; read-only by default |
| TFTP | octet-mode upload/download and temporary server; atomic local writes; read-only server by default |
| Syslog | UDP send/receive, priority extraction, sampled preview, and rotating JSONL files preserving original message bytes |

## Downloads and startup

[Historical release v0.1.0-alpha.1](https://github.com/lhuaizhong/HopKit/releases/tag/v0.1.0-alpha.1) · [SHA-256 checksums](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/SHA256SUMS.txt)

| Platform | Historical netbox package |
| --- | --- |
| Windows x64 | [Installer](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-setup.exe) |
| Windows x64 | [Portable EXE](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.exe) · [Portable ZIP with CLI](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-windows-x64-portable.zip) |
| macOS Apple Silicon | [DMG](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-arm64.dmg) |
| macOS Intel | [DMG](https://github.com/lhuaizhong/HopKit/releases/download/v0.1.0-alpha.1/netbox-0.1.0-alpha.1-macos-x86_64.dmg) |

The release page also provides macOS ZIPs with the CLI and per-file checksums. Historical archives retain their original executable names; the following names apply to current source builds.

- **Windows:** install the package or extract the ZIP into a writable local directory, then run `HopKit.exe`. The GUI requires [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/); the installer downloads it if missing. The installer and portable ZIP include the CLI; the single portable EXE contains only the GUI.
- **macOS:** choose the package for your CPU, open the DMG, and drag `HopKit.app` to Applications, or extract the app ZIP. The GUI uses the system WKWebView. ZIPs provide the CLI; release DMGs place it in `HopKit.app/Contents/Resources/`. See the [macOS guide](README-macOS.md).

Windows preview binaries are unsigned; macOS apps use ad-hoc signing without Apple notarization. Minimum OS versions, older Windows LTSC editions, and untested hardware are not certified. A successful build alone does not establish runtime compatibility.

Basic tools run as an ordinary user. Packages do not modify PATH, install persistent services, add firewall rules, or check for updates automatically. The CLI needs no WebView2, Node, Python, or development toolchain.

## GUI workflow

1. Review interfaces, addresses, gateways, DNS, and routes on the default **Troubleshooting** page. The overview refreshes every 15 seconds without public network probes.
2. Enter an IP, hostname, `host:port`, `[IPv6]:port`, or HTTP(S) URL. Review the normalized target; use advanced options for source IP, DNS comparison, timeout, traceroute, or an additional CA.
3. Start the check and inspect evidence, possible causes, and next steps. Use individual sidebar tools for further investigation.
4. Copy results or export JSON. After completion, **Export image / Copy image** captures displayed metrics, charts, tables, language, appearance, and filters as PNG. Images contain the retained preview; use Syslog files for complete received logs.

The GUI starts in English with the **Fresh** appearance. Switch **中文 / English** or **Appearance → Fresh / Easy to read** in the header; preferences are saved locally. Easy to read increases text size, contrast, and focus visibility. CLI output defaults to English; `--lang zh-CN` changes result headings. Machine-readable fields remain English; some preview help/errors remain bilingual.

The task center supports **4 concurrent tasks** and **40 session tasks**, with active tasks pinned above filterable history. Opening a task preserves the form; **Rerun** loads its saved request for review. Tool/mode parameters are retained separately during the session. Stop tasks individually or use **Stop all**; partial evidence is preserved. Closing the GUI stops its tasks while independent CLI processes continue. Task history and tool configurations do not persist across restarts.

Ping/TCPing statistics cover the entire task independently of the recent preview. Interrupted calls are counted separately and excluded from latency and failure rates. Charts show the latest 20/80/200 available samples; filters do not change cumulative metrics. Traceroute reports each hop separately: TTL expiry is a hop response, and an intermediate timeout alone does not establish a broken route.

## CLI usage

Multi-target monitoring: `hk monitor --target ping,192.168.1.1 --target ping,example.com --target tcping,business.example,443 --format jsonl`. Select **Monitor** in the GUI sidebar for a common timeline, pause/resume, notes, network events and before/after comparisons. Up to three targets are sampled sequentially using system routes; monitors share the existing four-task limit. Measurement gaps do not create failure samples. See [monitoring behavior, budgets and validation](MONITORING.md). Extended endurance, physical sleep/interface switching and macOS validation remain outstanding for this change.

Run from the extracted package directory:

| Platform | Full CLI | Equivalent short entry |
| --- | --- | --- |
| Windows PowerShell | `.\hopkit-cli.exe` | `.\shortcuts\hk.exe` |
| macOS Terminal | `./hopkit-cli` | `./shortcuts/hk` |

Examples use `hk`; replace it with the appropriate entry above, or manually add `shortcuts` to PATH after checking for an existing command. The short entry is an identical binary copy with the same arguments, output, and exit codes. New packages do not distribute `nb`. Replace example LAN addresses and file paths with your own.

### Diagnostics

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

Ping payload size is **0–65,500 bytes**, default **32**, excluding IP/ICMP headers; JSON records it as `payload_size_bytes`. Large packets depend on path MTU, fragmentation, and device policy.

### iperf3 network speed test

Start a server on one computer and a client on another. Both ends may use HopKit, or a supported standard iperf3 peer. The Rust implementation needs no external iperf3 executable, libiperf, or runtime download.

```sh
hk iperf3 serve --listen 0.0.0.0 --port 5201
hk iperf3 client 192.168.1.10 --duration 10 -P 4
hk iperf3 client 192.168.1.10 --duration 10 -R
hk iperf3 client 192.168.1.10 -u --bitrate-mbps 10 --format json
```

TCP supports **1–16 streams**; target rates apply per stream, with 0 meaning unlimited. UDP supports **one stream**, **1–10,000 Mbps**, and 1,200-byte client datagrams, plus TCP control traffic on the same port. Client duration is 1–3,600 seconds; server duration is 0–86,400 seconds, with 0 meaning until stopped. Servers handle one test at a time. Rates update every 0.5 seconds; cancellation preserves partial local statistics, while peer metrics require result exchange.

### File transfer and temporary services

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

For named FTP login, set `HOPKIT_FTP_PASSWORD` before using `--user`: PowerShell uses `$env:HOPKIT_FTP_PASSWORD='your-password'`; macOS uses `export HOPKIT_FTP_PASSWORD='your-password'`. Only an absent variable falls back to `NETBOX_FTP_PASSWORD`; an empty new value still takes precedence. Use `--password-env` for another variable. Passwords are excluded from caches, snapshots, and exports, and must be re-entered for reruns. Supply a host/IP without URL credentials.

### Output and exit codes

`--format json` emits one final object for finite tasks, including duration-limited services. Use `--format jsonl` for event streams and continuous operations. Services default to `--duration 0` and run until Ctrl+C. Trace uses `schema_version: 2`; other commands use version 1.

| Code | Meaning |
| --- | --- |
| `0` | Command completed and its applicable diagnostic criterion was met |
| `1` | Network or remote result did not meet the criterion |
| `2` | Invalid arguments |
| `3` | Local permissions, resource conflict, or unsupported capability |
| `4` | Local I/O or output failure |
| `130` | Cancelled; partial results are preserved when available |

`status=succeeded` means execution completed. Inspect `data.success` where present, the exit code, and individual evidence. A completed `check` can retain HTTP 403/500, certificate errors, or connection refusals. Ping/TCPing succeed with at least one successful probe; `--strict` requires zero loss.

## Usage notes and limitations

### Troubleshooting scope

Targets support IPs, internationalized domains, explicit ports, and HTTP(S) URLs. A bare host does not imply HTTP. URL credentials are rejected. Requests use the original query, but snapshots, JSON, PNG, and rerun parameters remove queries/fragments; re-enter the original URL when needed. Target handoff passes the host and relevant port to individual tools.

TCP/HTTP bind the selected source IP and record local/remote addresses. System DNS, specified DNS comparisons, and route queries follow system policy without source binding. Selecting a source skips ICMP/Trace, which do not support source binding. Link-local TCP requires numeric IPv6 interface scopes; named scopes and URL zone identifiers are unsupported. Source selection does not guarantee a fixed physical interface through route/VPN changes.

HTTP/TLS uses built-in HTTP/1.1, rustls, and Mozilla roots, without curl/OpenSSL programs, system proxies/certificate stores, or proxy/certificate environment overrides. It sends **HEAD only**, reads no body, and follows at most **5 redirects**. Credentials, non-HTTP(S) redirects, and HTTPS downgrades are rejected; no Cookie, Authorization, or Referer is sent. Headers are limited to 64 KiB, 128 fields, and 8 interim responses. HEAD and GET results may differ.

Validation is enabled by default. Add an enterprise PEM CA with `--ca-file` (maximum 1 MiB); snapshots record its path without file contents. `--insecure-tls` visibly marks chain, hostname, and time validation as skipped. Roots update with releases; online revocation checks and missing-intermediate retrieval are unsupported.

System DNS has a 5-second bound. Each TCP attempt and entire HTTP hop uses the selected timeout, with up to 8 candidate addresses; optional Trace is bounded to 120 seconds. Evidence timestamps use UTC; stage duration includes adapter overhead, with protocol timing recorded separately. Some ICMP/TCP/DNS calls may wait until timeout before cancellation. One session does not establish long-term availability or fault ownership.

### Temporary service configuration

Listeners default to `0.0.0.0` (all IPv4 interfaces); an empty allowlist on a non-loopback listener accepts all sources. Use `--listen` for a local IP, `::` for all IPv6 interfaces, or `127.0.0.1` / `::1` for local-only access. Restrict sources with `--allow` and comma-separated CIDRs. The GUI exposes the same settings in server/receive mode.

| Service | Default port | Alternative for testing |
| --- | --- | --- |
| FTP | TCP 21 | 2121 |
| NTP | UDP 123 | 1123 |
| TFTP | UDP 69 | 1069 |
| Syslog | UDP 514 | 1514 |
| iperf3 | TCP 5201; UDP tests also use UDP 5201 | Any available port |

Port access depends on permissions, occupancy, and firewall policy; services do not elevate privileges or switch ports automatically. FTP/TFTP servers are read-only by default; `--write` permits new files without replacing existing server files. Choose trusted roots without concurrent external path changes. FTP data connections and TFTP transfer sockets use OS-assigned ports that must be allowed through the firewall.

### Protocol and storage limits

- **NTP:** 1–20 client samples. The server uses the host clock, advertises stratum 10 by default (`--stratum 1..15`), and neither verifies upstream synchronization nor adjusts time. Only basic 48-byte unauthenticated NTPv3/v4 requests; no NTS, extensions, broadcast, peer, or control modes. Counters cover all received datagrams; previews are sampled.
- **iperf3:** no iperf2, authentication, SCTP, simultaneous bidirectional tests, warm-up, byte/block-count termination, parallel UDP, GSO/GRO, or custom MSS/window/TOS/congestion tuning. TCP retransmits and CPU usage are not measured; protocol CPU placeholders are not measurements.
- **FTP:** plaintext passive mode, EPSV with IPv4 PASV fallback, up to 4 control sessions, and LIST/NLST/MLSD within existing subdirectories. No FTPS/SFTP, active mode, resume, ASCII conversion, deletion, rename, or directory creation. Listings are limited to 10,000 entries / 4 MiB; symbolic links and unsafe filenames are hidden. Downloads replace local files only with `--overwrite` after a successful final reply. Other servers' upload replacement policies may differ; check remote files after interrupted uploads.
- **TFTP:** octet, 512-byte blocks, one active transfer, single-component filenames; maximum **33,553,919 bytes**. No option negotiation, block rollover, netascii, or resume. Blocks wait 1 second with up to 5 retries; receivers wait 2 seconds after committing. A lost final ACK can leave an uploaded file present despite a reported failure.
- **Syslog:** UDP delivery is not guaranteed. Preview sampling is approximately 20 messages/second after the first 10, with up to 200 visible events. Complete received messages go to `hopkit-syslog/`, preserving `raw_hex` and decoded text. Existing `netbox-syslog/` logs stay in place without automatic merging.
- **Log storage:** 64 MiB segments, 7-day / approximately 2 GiB retention, cleanup on segment closure, and up to one segment of temporary overrun. Writes synchronize every second and on normal shutdown, and stop below 1 GiB free space. Forced termination may leave a partial final line. Production throughput, strict crash recovery, SQLite indexing, and advanced queries are not guaranteed.

AI/MCP, automatic updates and update signature verification, persistent history, configuration import/export, and Syslog TCP/TLS are outside this preview. Telemetry is disabled; diagnostic probes require user action. This preview is not full acceptance of the original v0.3 production design.

## Development and verification

Use **Node 24** (`.nvmrc`: `24.14.0`) and **Rust 1.96.0** (`rust-toolchain.toml`), keeping `package-lock.json` and `Cargo.lock` intact. Desktop builds also need Visual Studio 2022 Build Tools on Windows, or Xcode Command Line Tools on macOS.

```sh
git clone https://github.com/lhuaizhong/HopKit.git
cd HopKit
npm ci
npm run build
cargo test -p hopkit-core --locked
cargo build -p hopkit-cli --locked
npm run desktop
```

For a Windows release build and local protocol regression:

```powershell
cargo build --release --locked --features hopkit-desktop/custom-protocol -p hopkit-desktop -p hopkit-cli
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
```

For macOS packaging and architecture checks, follow the [macOS guide](README-macOS.md). The [release workflow](.github/workflows/release.yml) defines package assembly and acceptance checks. Build metadata, dependency notices, SBOM, and SHA-256 files identify artifacts; historical toolchain versions belong to the corresponding `BUILD_INFO.json`.

Shared protocol regression uses Python's standard library and independent local peers without public network access. Additional checks include `scripts/diagnostic-trust-test.py`, `scripts/iperf3-test.py`, and `scripts/check-test.py`; inspect their platform/development prerequisites before running. UI tests use browser fixtures or test-only debugging; normal releases do not enable a debugging port. Local evidence goes to `test-output/`; packaged evidence, when included, is under `validation/`. Results apply to the recorded platform/artifact, without establishing coverage for other architectures, minimum OS versions, real devices, or changing VPN/multi-interface environments.

## Feedback and license

Submit bugs and feature requests through [GitHub Issues](https://github.com/lhuaizhong/HopKit/issues), including the app version, OS/architecture, parameters, expected/actual results, and exported JSON. Remove sensitive addresses, configuration, and log contents before sharing.

Copyright © 2026 Huaizhong Liu. Project source is available under the [MIT License](LICENSE). Third-party dependencies retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and release package notices.
