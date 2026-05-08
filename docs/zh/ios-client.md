# iOS 客户端构建与跨平台互联教程

这份教程用于把 SecureChat 跑到 iOS 模拟器或真机上，并让 iOS 客户端和 macOS 客户端通过同一个 relay 互发端到端加密消息。

## 当前能力

- iOS SwiftUI 原生客户端：登录/创建匿名身份、联系人列表、邀请链接复制、导入好友、聊天界面、设置页。
- 与 macOS 共用同一个 Rust 安全核心：`secure-chat-ffi`、X3DH 风格握手、Double Ratchet、邀请 URI、安全码、SQLite 本地会话状态、Apple Keychain 身份密钥。
- 与 macOS 共用同一个 relay API：HTTP(S) 和 QUIC relay URL 都使用相同密文信封和回执协议。
- 前台自动收取：App 运行时默认每 5 秒拉取一次离线密文和回执，并触发本地通知。
- 消息状态：`sent`、`delivered`、`read`、`received`、`failed` 与 macOS 客户端保持一致。

## 准备环境

需要 macOS 上安装：

- Xcode，包含 iOS Simulator SDK。
- Rust toolchain。
- 本仓库代码。

首次构建脚本会自动安装 Rust iOS targets：

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
```

## 构建 iOS 模拟器版本

在仓库根目录运行：

```bash
./script/build_ios.sh debug
```

脚本会完成三件事：

1. 编译 `secure-chat-ffi` 到 iOS 真机和 iOS Simulator 静态库。
2. 生成 `dist/SecureChatFFI.xcframework`。
3. 使用 Xcode 构建 `SecureChatIOS.app` 模拟器版本。

构建完成后打开工程：

```bash
open apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj
```

选择 `SecureChatIOS` scheme 和一个 iOS Simulator，然后运行。

## 真机运行

真机需要在 Xcode 里配置签名：

1. 打开 `apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj`。
2. 选择 `SecureChatIOS` target。
3. 在 `Signing & Capabilities` 中设置你的 Apple Development Team。
4. 把 Bundle Identifier 从默认的 `dev.local.securechat.ios` 改成你账号下唯一的 ID。
5. 先运行一次 `./script/build_ios.sh debug`，确保 `dist/SecureChatFFI.xcframework` 已生成。
6. 选择连接的 iPhone 或 iPad，点击 Run。

## 和 macOS 客户端互联

两端必须使用同一个 relay。公网部署可参考：

- [公共服务器部署指南](./public-server-deployment.md)
- [英文 relay 部署指南](../deploy-relay.md)

也可以先在局域网或本机启动测试 relay：

```bash
./script/run_relay.sh
```

互联流程：

1. 在 macOS 和 iOS 客户端登录页填写同一个 Relay URL，例如 `https://chat.example.com` 或 `quic://chat.example.com:443`。
2. 在其中一端进入邀请页，复制 `schat://invite/...` 邀请链接。
3. 把邀请链接发给另一端。
4. 另一端打开 Add Contact，粘贴邀请链接，填写显示名并保存。
5. 双方进入聊天页，比对安全码。安全码一致后说明当前身份公钥和设备摘要匹配。
6. 任意一端发送消息，另一端前台运行时会自动收取；也可以手动下拉或点击收取按钮。

跨平台互联不是通过 Swift 层重写协议实现的，而是 iOS 与 macOS 都调用同一个 Rust FFI JSON ABI。因此邀请、会话状态、双棘轮推进、relay 回执和安全码计算天然一致。

## iOS 数据存储

iOS 客户端数据位于 App 沙盒：

- Apple Keychain：身份密钥和本地消息存储密钥。
- Application Support 下的 `SecureChat.sqlite3`：联系人、会话状态、加密后的本地消息、relay 密文缓存。

删除 App 会删除本地 SQLite 数据；Keychain 项可能按系统策略保留，开发调试时可通过卸载 App、清理模拟器或更换 data directory 重新生成身份。

## 当前限制

- iOS 后台长期收取还没有接入 APNs。当前是 App 运行期间自动轮询，进入系统后台后不保证持续联网。
- 二维码扫描 UI 尚未接入相机；当前先使用邀请链接复制/粘贴完成加好友。
- 真机版本需要开发者证书签名后才能安装。
- 目前仍是 1 对 1 聊天，多设备 fan-out 和群聊 MLS 还在后续阶段。

## 排错

### 找不到 SecureChatFFI.xcframework

先运行：

```bash
./script/build_ios.sh debug
```

不要手动把 `dist/` 提交到 Git。它是本地构建产物。

### 模拟器可以构建，真机签名失败

在 Xcode 中设置 Apple Development Team，并修改 Bundle Identifier。脚本默认用 `CODE_SIGNING_ALLOWED=NO` 构建模拟器版本，所以不会替你配置真机签名。

### iOS 和 macOS 收不到对方消息

检查四件事：

1. 两端 Relay URL 完全一致。
2. relay `/health` 可访问。
3. 邀请链接来自对方当前身份。
4. 两端都在前台运行并手动点一次收取。

### QUIC 不通

先切到 HTTPS URL，例如：

```text
https://chat.example.com
```

确认 TCP 443 正常后，再排查 UDP 443、防火墙和证书配置。
