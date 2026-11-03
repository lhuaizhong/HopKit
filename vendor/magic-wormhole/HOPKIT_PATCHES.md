# HopKit modifications to magic-wormhole

Upstream: crates.io magic-wormhole 0.8.1 (release source; upstream revision 13e1cf7).
Original published .crate SHA-256: `77f48df27b95c28b11b82c1db97d4a95964ef164da4288bac4acc4893329190d`.
Date: 2026-11-03. License: EUPL-1.2; see LICENSE (retained verbatim).

Modified source files:
- `src/transit/transport.rs`: reject ciphertext records above 1 MiB before allocation; validate STUN response length; use bounded async DNS for STUN and relay.
- `src/bounded_dns.rs`, `src/lib.rs`: four shared off-thread system resolvers maximum, five-second deadline; timeout/cancellation drops the awaiting future without blocking the executor. Outstanding system DNS calls retain their slot until they return.
- `src/core/rendezvous.rs`: bounded DNS and async TCP, preserving the original hostname for native TLS certificate/hostname verification.
- `src/transfer/v1.rs`, `src/transfer.rs`: checked u64 remaining length, reject empty/oversized file records, check ack value alongside checksum; additive send_file_observed / accept_observed callbacks (existing APIs retained).
- `src/transfer/cancel.rs`: cleanup budget two seconds; additive observed cleanup reports warning separately from successful delivery.

No cryptography or on-wire message formats were changed. Existing upstream tracing is not enabled by HopKit.
Local regression commands: `cargo test --workspace --locked`, `python3 scripts/wormhole-test.py --python-bin <venv>/bin`, `node scripts/wormhole-ui-test.mjs`; optional `--large` sends an actual >4 GiB file through local services. Native test instructions: scripts/wormhole-native-test.py.

This is a modified source copy. Test reports in test-output/wormhole describe exactly which platform and scenarios were executed; cross-platform and public-network checks require separate execution.

Patch boundary tests: `cargo test --manifest-path vendor/magic-wormhole/Cargo.toml --lib --no-default-features --features transfer,native-tls hopkit_tests`. The vendor Cargo.lock is retained for standalone upstream test builds; the application dependency graph is locked by the workspace Cargo.lock.
