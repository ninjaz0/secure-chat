# SecureChat

SecureChat 是一个可自托管的端到端加密聊天原型。项目包含 Rust 安全核心、
Rust relay 服务器、原生 macOS SwiftUI 客户端、原生 iOS SwiftUI 客户端和
原生 Android 客户端；各端通过同一套 Rust FFI 运行时和 relay 协议互通。

当前项目已经可以用于自测和小范围试用，但它仍是“可部署原型”，不是经过第三
方密码学审计的安全通信产品。不要在外部审计前把它宣传为“绝对安全”。

## 主要能力

- X3DH-style 异步建联和 Double Ratchet 消息层。
- OpenMLS/RFC 9420 ciphersuite-oriented 群聊原型：Welcome 控制消息、
  epoch 轮换、群消息加密 fan-out、relay MLS KeyPackage 发布和领取 API。
- 匿名账号、每设备身份密钥、邀请链接、安全码。
- 默认使用 ChaCha20-Poly1305 消息加密，并保留 AES-256-GCM suite enum。
- HTTPS 与 QUIC relay 传输，共用同一套加密 frame API。
- 设备注册、发送、拉取、送达/已读回执、MLS 私有操作和 APNs push 操作都使用
  Ed25519 relay request signature。
- relay 使用 SQLite 持久化公开 pre-key bundle、离线密文队列、回执、
  MLS KeyPackage 和 APNs device token。
- macOS/iOS 使用 Apple Keychain 保存身份密钥和本地存储密钥。
- Android 使用 app-private no-backup 存储，并排除 Android cloud backup 和
  device-transfer extraction。
- 本地 SQLite 保存联系人、加密 ratchet session、加密消息正文和缓存的 relay
  密文。
- macOS、iOS、Android 客户端支持登录、联系人、邀请导入/复制、聊天记录、
  relay 设置、群聊、临时会话、后台轮询、通知和 sent/delivered/read 状态展示。
- v0.2.5 起，三端聊天 UI 支持联系人昵称编辑和本地强删除、文件/图片发送、
  表情图片导入与发送、Unicode emoji 输入显示、阅后即焚消息，以及类似主流
  聊天软件的“贴底才自动滚动”行为。
- 附件和表情图片通过端到端加密内容载荷发送。实现不设置产品级单文件上限，但
  仍受 relay 单条密文约 1MB、队列、磁盘、网络和系统资源约束；失败会在本地
  传输状态里展示，可在后续版本扩展重试 UI。

## 快速构建

构建并启动 macOS 客户端：

```bash
./script/build_and_run.sh --verify
```

只构建不启动，适合 CI 或快速检查：

```bash
./script/build_and_run.sh --build-only
```

构建 iOS 模拟器客户端，并把 Rust FFI 静态库打包成 XCFramework：

```bash
./script/build_ios.sh debug
open apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj
```

iOS 工程依赖 `dist/SecureChatFFI.xcframework`，该文件由脚本从
`secure-chat-ffi` C ABI 生成，和 macOS 客户端使用的是同一个安全核心。

重新生成 app 图标：

```bash
./script/generate_app_icons.py /path/to/source-icon.png
```

## 发布包构建

构建本地 macOS DMG：

```bash
./script/package_macos.sh
```

产物会写到 `dist/SecureChatMac-0.2.5.dmg`。默认使用 ad-hoc 签名，适合本地
测试。正式发布时需要设置 Developer ID 身份并启用严格闸门：

```bash
SECURE_CHAT_MACOS_SIGN_IDENTITY="Developer ID Application: Example Inc (TEAMID)" \
SECURE_CHAT_RELEASE_STRICT=1 \
./script/package_macos.sh
```

严格模式会拒绝 ad-hoc 签名，并运行 Gatekeeper assessment。真正公开发布还需
要 Apple notarization。

构建 Android release APK：

```bash
./script/build_android.sh release
```

发布自动化会把最终可安装 APK 复制为 `dist/SecureChatAndroid-0.2.5.apk`，并和
macOS DMG 一起生成 SHA-256 校验文件。

如需生成正式签名 APK，设置以下环境变量：

```bash
SECURE_CHAT_ANDROID_KEYSTORE=/path/to/release.keystore \
SECURE_CHAT_ANDROID_KEYSTORE_PASSWORD=... \
SECURE_CHAT_ANDROID_KEY_ALIAS=securechat \
SECURE_CHAT_ANDROID_KEY_PASSWORD=... \
./script/build_android.sh release
```

发布自动化中建议额外设置：

```bash
SECURE_CHAT_REQUIRE_RELEASE_SIGNING=1
```

这样脚本会拒绝 unsigned 或 debug-signed APK。若脚本需要下载 Gradle，会先校验
固定的 SHA-256，再解压使用。

## 测试与本地验证

运行 Rust 全量测试：

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace
```

运行一对一 relay E2EE smoke：

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
```

运行群聊/APNs token smoke：

```bash
SECURE_CHAT_SMOKE_MODE=group PATH="$HOME/.cargo/bin:$PATH" \
  cargo run -p secure-chat-client --bin secure-chat-smoke
```

运行 P2P rendezvous smoke：

```bash
SECURE_CHAT_SMOKE_MODE=p2p PATH="$HOME/.cargo/bin:$PATH" \
  cargo run -p secure-chat-client --bin secure-chat-smoke
```

依赖安全审计：

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo audit --deny warnings
```

## 启动 relay

启动本地 HTTP relay：

```bash
./script/run_relay.sh
```

使用证书启动 HTTPS 和 QUIC listener：

```bash
SECURE_CHAT_RELAY_HTTP_ADDR=127.0.0.1:8787 \
SECURE_CHAT_RELAY_HTTPS_ADDR=0.0.0.0:443 \
SECURE_CHAT_RELAY_QUIC_ADDR=0.0.0.0:443 \
SECURE_CHAT_TLS_CERT=/etc/secure-chat/tls/fullchain.pem \
SECURE_CHAT_TLS_KEY=/etc/secure-chat/tls/privkey.pem \
SECURE_CHAT_RELAY_DB=/var/lib/secure-chat/relay.sqlite3 \
./script/run_relay.sh
```

生产 relay 可选 APNs provider 配置：

```bash
SECURE_CHAT_APNS_TEAM_ID=TEAMID1234 \
SECURE_CHAT_APNS_KEY_ID=KEYID12345 \
SECURE_CHAT_APNS_PRIVATE_KEY_PATH=/etc/secure-chat/apns/AuthKey_KEYID12345.p8 \
SECURE_CHAT_APNS_TOPIC_IOS=com.example.securechat \
SECURE_CHAT_APNS_TOPIC_MACOS=com.example.securechat.mac \
SECURE_CHAT_APNS_ENV=production \
./script/run_relay.sh
```

APNs payload 是通用提醒，不包含联系人名、群名、消息正文或密文本身。如果未配置
APNs，或 APNs 投递失败，客户端仍会通过轮询收消息。

## 公网部署

Ubuntu 22.04/24.04 LTS 推荐使用一键部署脚本。

无域名时使用服务器公网 IP：

```bash
./deploy/install-relay.sh --email you@example.com
```

有域名时：

```bash
./deploy/install-relay.sh --domain chat.example.com --email you@example.com
```

安装完成后，服务器上会有维护命令：

```bash
chatrelay
```

也可以直接执行：

```bash
chatrelay status
chatrelay logs
chatrelay restart
chatrelay health
chatrelay backup
chatrelay update
chatrelay renew
```

relay 安装和 `chatrelay update` 都使用 `cargo --locked` 构建，并写入
`/etc/secure-chat/build-info.txt`，记录 git revision、`Cargo.lock` hash、
二进制 hash 和 Rust 工具链版本。

更多部署和使用文档：

- 英文 relay 部署指南：[docs/deploy-relay.md](docs/deploy-relay.md)
- 中文公共服务器部署指南：[docs/zh/public-server-deployment.md](docs/zh/public-server-deployment.md)
- 中文客户端安装与首次使用说明：[docs/zh/client-installation.md](docs/zh/client-installation.md)
- 中文客户端使用教程：[docs/zh/usage-guide.md](docs/zh/usage-guide.md)
- 中文 iOS 构建与互联教程：[docs/zh/ios-client.md](docs/zh/ios-client.md)
- 生产环境变量示例：[deploy/relay.env.example](deploy/relay.env.example)
- v0.2.5 发布说明：[docs/releases/v0.2.5.md](docs/releases/v0.2.5.md)
- 安全审计报告：[docs/zh/security-audit-2026-05-09.md](docs/zh/security-audit-2026-05-09.md)

## 聊天能力

- 一对一聊天、群聊和临时会话都使用同一套版本化内容载荷，旧文本消息仍通过
  `body` fallback 显示。
- 文本消息不转义或过滤 Unicode emoji，直接使用系统输入法/键盘输入。
- Composer 提供文件、图片、阅后即焚和表情入口；导入的表情图片保存在本地私有
  目录，发送时作为轻量图片消息传给对方，对方不需要拥有同一套表情包。
- 图片和表情消息显示缩略图，文件消息显示文件名、大小和传输状态。
- 阅后即焚消息打开后会立即本地销毁并发送销毁通知；收到销毁通知的设备也会隐藏
  对应内容并清理本地附件文件。
- 用户位于聊天底部附近时，新消息会自动跟随到底部；回看历史消息时，新消息只显示
  轻量提示，点击提示后回到最新。用户自己发送消息始终滚到最新。
- 删除联系人是本地强删除：联系人、1:1 消息、会话密钥和相关未完成附件都会删除。
  对方未来再次发来消息时，会重新作为未命名未验证联系人出现。

## 两人聊天流程

1. 部署或启动一台 relay。
2. 每个客户端都填写同一个 Relay URL，例如 `https://chat.example.com` 或
   `quic://chat.example.com:443`。
3. 用户 A 在客户端里创建邀请链接。
4. 用户 B 在 Add Contact 中导入邀请。
5. 双方通过可信的外部渠道比对安全码或二维码。
6. 开始发文本、emoji、图片、文件、表情或阅后即焚消息。客户端会后台轮询、展示
   通知，并根据 relay 回执更新 sent/delivered/read 状态。

relay 只接收公开 pre-key bundle、不透明密文 frame 和送达/已读回执。私有 relay
操作都由设备签名，所以攻击者不能只靠猜测 device ID 拉取别人的队列。明文只在
端点运行时内出现。

## 群聊流程

1. 在 macOS、iOS 或 Android 客户端创建群聊。
2. 从已有联系人中添加成员设备。当前版本按设备身份建模，一个人的多设备不会自动
   合并成单一用户。
3. invite/Welcome 控制消息通过已有一对一 E2EE 通道发送。
4. 群消息按当前 group epoch 加密，再作为不透明密文分别排入每个成员设备的
   relay 队列。
5. relay 支持通过 `/v1/mls/key-packages` 发布 signed MLS KeyPackage，并通过
   `/v1/mls/key-packages/claim` 领取。领取接口要求请求者与目标设备存在关系授权，
   避免任意注册设备消耗他人的 KeyPackage。

## 项目结构

- `crates/secure-chat-core`：身份密钥、pre-key bundle、邀请链接、
  X3DH-style session setup、Double Ratchet、OpenMLS ciphersuite-bound 群聊状态、
  安全码、relay API 类型和 padded transport frame。
- `crates/secure-chat-client`：HTTP(S)/QUIC relay client、内存安全设备运行时、
  基于邀请的 session 创建、加密发送、拉取、回执和解密流程。
- `crates/secure-chat-desktop`：面向桌面和移动 FFI 的 runtime，包含 Keychain
  身份存储、SQLite 联系人、加密 session、加密群组、加密消息、附件/表情本地
  索引、阅后即焚状态和远端消息 ID。
- `crates/secure-chat-relay`：Axum HTTPS relay、Quinn QUIC relay、共享状态、
  SQLite 持久化、密文队列和回执队列。
- `crates/secure-chat-ffi`：供 SwiftUI 和 Android JNI 调用的 C ABI。
- `apps/macos/SecureChatMac`：原生 macOS SwiftUI 客户端。
- `apps/ios/SecureChatIOS`：原生 iOS SwiftUI 客户端，链接
  `dist/SecureChatFFI.xcframework`。
- `apps/android/SecureChatAndroid`：原生 Android Kotlin/Compose 客户端，通过 JNI
  调用同一套 Rust FFI。

## 协议快照

- 身份：匿名 account ID，每设备 Ed25519 signing key 和 X25519 identity key。
- 认证：account signing key 签名每个 device identity，device signing key 签名
  当前 signed pre-key。
- 会话建立：X3DH-style X25519 组合，覆盖 identity key、signed pre-key、ephemeral
  key 和可选 one-time pre-key。
- 消息安全：Double Ratchet，使用 X25519 DH ratchet 和 HKDF-SHA256 chain ratchet。
- AEAD：默认 ChaCha20-Poly1305，保留 AES-256-GCM suite enum。
- Header protection：消息序号和 content type 被 AEAD 保护，ratchet 恢复字段保持
  authenticated cleartext。
- Content payload：本地 runtime 使用版本化 JSON 载荷承载文本、图片、文件、
  表情、阅后即焚内容和销毁通知；旧纯文本 `body` 继续作为兼容字段。
- Attachments：文件、图片和表情项会记录 metadata、sha256、本地路径和传输状态。
  relay 仍只看到 Double Ratchet/群聊加密后的不透明密文分片。
- 邀请链接：当前版本会签名完整 invite metadata，包括 mode、过期时间、relay hint
  和 bundle transcript hash；旧版未签名邀请会被拒绝，需要重新生成。
- OOB verification：安全码和 QR payload 由双方 account/device 公钥摘要派生。
- Transport：HTTPS 和 QUIC 携带同一类 E2EE ciphertext envelope；核心还支持签名
  P2P UDP rendezvous、NAT candidate probing、固定大小 padding、jitter profile 和
  cover-traffic flag。
- Relay auth：设备 Ed25519 签名绑定 action、request digest、timestamp、nonce、
  account ID 和 device ID；relay 会拒绝未签名、过期和重放的私有命令。持久化 relay
  会把 auth nonce 写入 SQLite，重启后不会重新打开时间窗口内的重放风险。
- 群聊：当前是 OpenMLS ciphersuite-oriented group prototype，不等同于完整
  RFC 9420 MLS state machine。群聊会持久化每设备成员和 epoch secret，成员变化时
  轮换 epoch，并通过每设备 relay 队列发送不透明群密文。
- Push：APNs device token 通过签名 relay 请求注册。relay 只发送通用的
  `New encrypted message` 通知和刷新提示。

## 当前限制

- 群聊仍是 per-device 模型，不会把一个人的多个设备自动聚合成单一用户。
- 群聊层还不是完整 MLS state machine；高风险群聊场景需要后续接入真实 OpenMLS
  credential/proposal/commit/welcome 流程。
- 附件发送已经使用本地分片/重组模型，但 relay 仍通过现有密文队列承载分片，
  大文件体验受队列长度、磁盘和网络条件影响。
- Android 还没有接入 Android Keystore/StrongBox wrapping key；目前依赖
  app-private no-backup 文件权限和本地加密。
- P2P NAT traversal 已支持签名 UDP rendezvous 和直连探测，复杂 NAT 下会回退到
  relay。
- macOS 和 iOS 已有 APNs 注册路径，但真实后台推送需要 Apple Developer signing、
  push entitlement、bundle topic 和 relay APNs provider secret。
- 真机 iPhone/iPad 安装需要在 iOS Xcode 工程中设置 Apple development team 和
  bundle identifier。
- relay 有 durable SQLite 队列，但没有做水平复制。
- 公开安全声明前仍需要第三方密码学和实现审计。
