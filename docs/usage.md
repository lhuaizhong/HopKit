# HopKit user guide

[Back to README](../README.md) · [简体中文](usage.zh-CN.md)

This guide covers the current source tree. See the README for installation and quick start; published packages may not include every new feature.

[Interface and results](#interface-tasks-and-results) · [Monitoring](#multi-target-monitoring) · [CLI](#cli-reference) · [HTTP sharing](#http-file-sharing) · [Wormhole](#wormhole) · [Limits](#usage-limits) · [Verification](#verification)

## Interface, tasks, and results

The GUI starts in English with the **Fresh** appearance. Switch **中文 / English** or **Appearance → Fresh / Easy to read** in the header; preferences are saved locally. Easy to read increases text size, contrast, and focus visibility. CLI output defaults to English; `--lang zh-CN` changes result headings. Machine-readable fields remain English; some preview help/errors remain bilingual.

The task center supports **4 concurrent tasks** and **40 session tasks**, with active tasks pinned above filterable history. Opening a task preserves the form; **Rerun** loads its saved request for review. Tool/mode parameters are retained separately during the session. Stop tasks individually or use **Stop all**; partial evidence is preserved. Closing the GUI stops its tasks while independent CLI processes continue. Task history and tool configurations do not persist across restarts.

Ping/TCPing statistics cover the entire task independently of the recent preview. Interrupted calls are counted separately and excluded from latency and failure rates. Charts show the latest 20/80/200 available samples; filters do not change cumulative metrics. Traceroute reports each hop separately: TTL expiry is a hop response, and an intermediate timeout alone does not establish a broken route.

Windows TCPing prefers the TCP stack's RTT from `SIO_TCP_INFO`, excluding application scheduling and first-use overhead. Unsupported or invalid statistics and SYN retransmissions fall back to measured connection time. Successful samples export `rtt_source` (`tcp_info` or `connect_elapsed`) and preserve the full connection call duration in `connect_elapsed_ms`. Each probe opens its own connection; the first sample is retained.

## Multi-target monitoring

Select **Monitor** in the GUI sidebar and configure up to three ICMP/TCP targets. Use the common timeline to review failure/recovery periods, notes, and before/after comparisons.

```sh
hk monitor --target ping,192.168.1.1 --target ping,example.com --target tcping,business.example,443 --format jsonl
```

Targets are sampled sequentially using system routes. Each monitor occupies one task slot, sharing the four-task limit with other tools. Pauses, sleep, and sampling gaps do not create failure samples. Monitoring beyond two hours and physical sleep/interface switching still require validation.

## CLI reference

Run from the extracted package directory:

| Platform | Full CLI | Equivalent short entry |
| --- | --- | --- |
| Windows PowerShell | `.\hopkit-cli.exe` | `.\shortcuts\hk.exe` |
| macOS Terminal | `./hopkit-cli` | `./shortcuts/hk` |

Examples use `hk`; replace it with the appropriate entry above, or manually add `shortcuts` to PATH after checking for an existing command. The short entry is an identical binary copy with the same arguments, output, and exit codes. Replace example LAN addresses and file paths with your own.

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

## HTTP file sharing

Select **HTTP**, directly below FTP in the sidebar, choose an existing directory, and start the service. The default port is **80** and can be changed. Open or copy an interface URL for another device. The IP and port must be reachable from that device.

Browsers can navigate subdirectories and download files without additional software. Uploads are disabled by default. Enable **Allow file uploads** before starting to upload multiple files to the current directory, with progress and cancellation. Existing files are never overwritten; interrupted uploads are cleaned up. Transfers stream between disk and socket and support UTF-8 filenames, IPv4/IPv6, HEAD and download ranges.

Source CIDRs and an optional service duration are available. Stopping the service or closing HopKit closes listeners and active transfers while retaining transfer statistics. This temporary server uses plain HTTP without login or HTTPS. Dotfiles and symlinks are excluded. On macOS, use a port such as 8080 if port 80 requires elevated privileges.

```sh
hk http serve --root ./shared
hk http serve --root ./shared --port 8080 --write
hk http serve --root ./shared --listen :: --port 8080 --allow ::1/128 --duration 300 --format jsonl
```

## Wormhole

Wormhole transfers one regular file using a one-time pairing code. Select **Wormhole** in the sidebar to send or receive. The receiver chooses a destination and explicitly accepts the offer. Existing files are never replaced. Directory offers from Python peers are saved as ZIP files without extraction. Both peers must stay online; resume and offline delivery are unsupported.

```sh
hk wormhole send ./report.zip
hk wormhole receive --dir ./received
hk wormhole receive --code-stdin --dir ./received --yes --format jsonl
hk wormhole send ./report.zip --connection relay --format jsonl --emit-code
```

The receive directory must already exist. An interactive receiver prompts for the pairing code and acceptance. JSON/JSONL receive requires explicit `--yes`; use `--code-stdin` to avoid passing the code as a command-line argument. `--verify` is available only in an interactive text terminal.

### Connections and privacy

- Public WSS Mailbox and Transit Relay services are used by default, with connections starting only when a transfer is started. Custom services are supported.
- `--connection auto` selects a connection automatically; `direct` allows only direct data transfer but still needs signaling; `relay` uses neither STUN nor direct listeners.
- File data is end-to-end encrypted. Signaling, relay, and STUN services still observe connection metadata; direct peers learn each other's addresses.
- Received pairing codes are excluded from history and exports, and reruns require a new code. Machine output omits codes by default. Send codes go to stderr; `--emit-code` explicitly includes the sensitive code once in sender JSONL output.
- Wormhole uses the native system TLS trust store, unlike HTTP diagnostics. TLS verification cannot be bypassed.

### Interrupted transfers

Normal cancellation cleans up uncommitted `.hopkit-*.part` files. Forced termination or power loss may leave temporary files beside the destination; remove them after the transfer has stopped. A committed file is retained even if receipt delivery fails.

This feature is awaiting release acceptance. See the [implementation record](../WORMHOLE_IMPLEMENTATION.md) for completed and outstanding checks, and [distribution licensing](../DISTRIBUTION_LICENSES.md) for EUPL-1.2 obligations.

## Usage limits

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
| HTTP | TCP 80 | 8080 |
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

## Verification

Shared protocol regression uses Python's standard library and independent local peers without public network access. After building the release CLI, run from the project root on Windows:

```powershell
python scripts/integration_test.py --exe target/release/hopkit-cli.exe
```

On macOS, use `target/release/hopkit-cli`. Additional checks include `scripts/diagnostic-trust-test.py`, `scripts/iperf3-test.py`, and `scripts/check-test.py`; inspect their platform/development prerequisites before running. Wormhole test prerequisites and release gates are in the [release instructions](../RELEASING.md).

UI tests use browser fixtures or test-only debugging; normal releases do not enable a debugging port. Local evidence goes to `test-output/`; packaged evidence, when included, is under `validation/`. Paths are relative to the project or package root. Results apply to the recorded platform/artifact; other architectures, minimum OS versions, real devices, and changing VPN/multi-interface environments require their own checks.
