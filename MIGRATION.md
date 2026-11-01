# Upgrading from netbox / 从 netbox 升级

HopKit `0.1.0-beta.1` is the first HopKit Pre-release. The earlier
`v0.1.0-alpha.1` tag and netbox downloads are preserved.

| Item / 项目 | Current / 当前 | Compatibility / 兼容说明 |
| --- | --- | --- |
| GUI | `HopKit.exe` / `HopKit.app` | Install or copy the new app / 安装或复制新应用 |
| CLI | `hopkit-cli.exe` / `hopkit-cli` | Replace old script executable paths / 更新脚本中的程序路径 |
| Short entry / 短入口 | `shortcuts/hk.exe` / `shortcuts/hk` | Exact CLI copy; no automatic PATH edit / 不自动修改 PATH |
| FTP password / 密码 | `HOPKIT_FTP_PASSWORD` | Falls back to `NETBOX_FTP_PASSWORD` only if the new variable is absent / 仅新变量不存在时回退旧变量 |
| Syslog directory / 日志目录 | `hopkit-syslog/` | Existing `netbox-syslog/` files remain in place / 旧日志保留原处 |
| GUI preferences / 界面偏好 | HopKit language and appearance keys | Legacy preferences are read when new keys are absent / 缺少新键时读取旧偏好 |

Close the old GUI and stop its temporary services before starting the new version.
Keep existing exports and logs. New and old log directories are not merged
automatically. Check command collisions before manually adding `shortcuts` to PATH.
The packages no longer distribute `nb`.

升级前关闭旧 GUI 并停止临时服务，保留已有导出与日志。新旧日志不会自动合并；
手动添加 PATH 前请检查 `hk` 命令冲突。新包不再分发 `nb`。

The application identifier remains `local.netbox.network-toolkit.preview` to retain
the existing application data location. Session task history is not persistent.

应用标识暂保留 `local.netbox.network-toolkit.preview`，以沿用已有应用数据位置；
会话任务历史不跨重启保存。
