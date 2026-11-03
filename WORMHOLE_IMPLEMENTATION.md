# Wormhole 实现记录与验收范围

日期：2026-11-03。开发基于 HopKit 0.1.0-rc.1，功能尚未对外发布。

已实现共享 Rust File Transfer v1 核心、GUI 发送/接收和 CLI 子命令。
导航顺序为 FTP → HTTP → Wormhole → Syslog。默认配对码为数字前缀 + 2 个单词（按用户后续调整），支持一次性随机配对码、完整校验字符串、手动接受/拒绝、进度/速度/ETA、取消、任务中心确认提示、中英文及两种外观。文件流式 SHA-256；接收端 flush/sync/关闭句柄并禁止覆盖提交后才向对方发送回执。配对码不进入公开状态、历史请求、JSON、通用复制和图片导出。接收码不复用。

上游固定为 magic-wormhole 0.8.1 + HopKit 补丁，使用 native-tls 系统信任库；补丁与 EUPL 原文见 vendor/magic-wormhole。自定义服务和连接策略边界见 README。所有测试对端和 Python 服务均仅用于开发，不进入应用运行依赖。

## 已执行验证

实测机器：macOS 27.0.1 / Apple Silicon arm64；Rust 1.96.0；最终前端构建使用 Node 24.22.1。

- npm run build 通过；偏好迁移、监测范围回归通过。
- cargo test --workspace --locked：核心 51 项；CLI 3 项；桌面 1 项；已有集成测试 28 项，共 83 项通过（含 FTP/HTTP/TFTP 生命周期等）。
- vendor 补丁边界 3 项：超大密文 record 在分配前被拒绝、短 record 错误、空明文 record/剩余长度下溢/u64 大小边界。
- 本地信令与仅中继协议测试：空文件、中文/空格名称、8 MiB、官方 Python 0.23.0 双向、两词/四词码、Python 目录 ZIP 原样保存、等待配对取消、同名冲突、提交竞争与中途取消。
- 实际 4,294,968,320 字节（4 GiB + 1 KiB）通过本地中继；流式哈希一致。100 ms 采样峰值 RSS：发送端 26,576 KiB、接收端 26,720 KiB；该数值为采样结果，不是操作系统精确最高水位。
- 浏览器 fixture：菜单顺序、开始前无请求、私密码显示、接收码清除、JSON 脱敏、offer/公告文本转义、接受操作、图片导出、中英文、960/1220 宽度与两种外观。已查看生成截图；侧栏在最小窗口可滚动。
- 原生 WKWebView 专用调试构建：真实 Vue 发送表单、Tauri IPC、私密查询、真实接收落盘/哈希/发送方回执、完整校验字符串一致、路径冲突保留待确认并可拒收、发送端收到明确拒收、四任务上限、第 5 项被拒绝、stop_all 两秒内完成、结束后再次启动。测试功能仅存在于 native-test 调试构建；release 启用该 feature 会被编译拒绝。
- 默认公共服务小文件 smoke：WSS Mailbox + 公共 Transit；同一电脑的强制中继和自动模式成功，自动模式实际为直连。这不替代不同网络验收。
- 已生成并核对本地 SBOM（401 项）、vendor EUPL 原文/修改记录和对应源码归档 SHA-256；发行包仍需最终核对。
- 优化版 CLI 和桌面构建通过；Intel macOS CLI 交叉编译通过。

原始结果、截图和导出图在 test-output/wormhole/（不纳入源码版本控制）。
可复现命令见 scripts/wormhole-test.py、wormhole-ui-test.mjs、wormhole-native-test.py、wormhole-public-smoke.py 的文件说明。公共 smoke 必须显式 --run-public-smoke；大文件测试显式 --large。默认本地测试不会使用公共信令、中继或 STUN。

## 仍需发行前验收

- Windows x64 编译/原生运行、Intel macOS 桌面/原生运行以及 Windows ↔ macOS 双向。
- 两端位于不同网络时的公共服务自动/仅中继测试。
- 原生系统文件选择/保存对话框的人工操作、物理磁盘耗尽/权限变化和突然断电；自动原生脚本使用真实 IPC 指定本地路径，没有代替对话框人工验收。
- 回执或 Mailbox 关闭丢失的网络故障注入、各阶段完整超时与极端 DNS 挂起测试。
- 正式安装包、签名、安装/卸载、确切源码归档与二进制对应关系以及最终组合分发许可核对。

上述项目没有标为通过。开发代码保留原版本号；本地构建不代表完成三平台发布验收。源码分发工具已纳入 vendor 许可、补丁、锁文件、构建脚本和对应源码归档，详见 DISTRIBUTION_LICENSES.md 和 RELEASING.md。
