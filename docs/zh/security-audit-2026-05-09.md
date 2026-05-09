# SecureChat 安全审计报告

> 说明：本报告记录的是 `v0.2.4` 发布时的本地自查和 artifact hash。`v0.2.5`
> 新增聊天能力后的发布验证见仓库根 README 和 `docs/releases/v0.2.5.md`。

审计日期：2026-05-09
审计对象：SecureChat `v0.2.4` 当前工作区
审计范围：本地静态审计、本地测试、本地构建产物签名检查、当前未提交中文客户端安装文档
审计边界：未攻击真实公网服务器，未做形式化密码学证明，未修改业务代码

## Executive Summary

总体风险评级：High。

SecureChat 已具备一批重要安全基础：设备身份和 pre-key bundle 有签名校验，relay 私有操作使用 Ed25519 请求签名，Double Ratchet 消息层使用 AEAD 绑定 header/body AD，relay 会拒绝未签名、重放、越权 receipt、超大 ciphertext 和未授权 P2P candidate 读取。Rust 全量测试、relay/P2P/group smoke、`cargo audit` 和 Android release lint/unit 均已在本地跑过。

但按“高风险通信”标准，目前不建议作为公开生产安全通信工具发布给非测试用户。最优先需要处理的 5 个风险是：

1. Android 发布 APK 使用 Android Debug 证书签名，发布者身份和升级链不可被用户可靠验证。
2. macOS DMG 内 app 仅 ad-hoc 签名且未 notarize，Gatekeeper 会拒绝。
3. invite link 中的 `mode`、`expires_unix`、`relay_hint` 未纳入认证，临时邀请语义可被中间人改写。
4. relay 开放注册缺少 IP/account admission control，MLS KeyPackage claim 缺少关系授权，公开服务有明显 DoS 面。
5. CI/release 流程未强制执行 `cargo audit`、Android lint、APK/DMG 签名策略和 artifact hash 校验。

本轮未发现可直接解密历史聊天明文的 Critical 级漏洞；发现 High 5 条、Medium 6 条、Low 2 条、Info 1 条。

### 审计后加固状态

以下为 2026-05-09 审计后的本地修复状态，原始 finding 证据保留为当时快照：

- 已落地：`SC-H-001` Android release signing gate/R8/backup rules、`SC-H-002` macOS Developer ID strict gate、`SC-H-003` invite 元数据签名、`SC-H-004` relay 注册速率控制、`SC-H-005` MLS KeyPackage 关系授权、`SC-M-001` relay auth nonce SQLite 持久化、`SC-M-002` Android data extraction/backup 规则与 release hardening、`SC-M-006` CI 安全门禁、`SC-L-001` Gradle checksum 与 relay `cargo --locked`/build-info 可追溯性。
- 部分缓解但未彻底关闭：`SC-M-005` iOS/macOS 仍需正式 bundle ID、Apple Team、production APNs entitlement 和 notarization 账号配置。
- 仍需专项实现：`SC-M-003` Android Keystore/StrongBox wrapping key、`SC-M-004` 完整 MLS state machine 或进一步降级群聊安全声明、`SC-L-002` AndroidX/Compose 依赖升级回归测试。

## Threat Model

### 受保护资产

- 设备身份私钥、account signing key、device signing key、X25519 identity/signed pre-key/one-time pre-key。
- 一对一明文消息、群聊明文消息、本地消息数据库和 ratchet session 状态。
- 安全码、invite URI、临时聊天身份边界、群聊成员列表和 epoch secret。
- relay 中的 pre-key bundle、离线 ciphertext queue、receipt queue、MLS KeyPackage、APNs token。
- 发布产物、签名证书、release hash、部署脚本和服务器 TLS 私钥。

### 攻击者能力

- 主动网络攻击者：可拦截、重放、延迟、替换 relay HTTP/QUIC/P2P 流量。
- 恶意或被攻陷 relay：可观察 relay 收到的 ciphertext、metadata、APNs token、KeyPackage、队列状态，可尝试重放已签名请求。
- 任意注册客户端：可生成合法账号和设备身份，调用公开 relay API。
- 本地低权限攻击者或备份/迁移路径：可尝试读取 app 数据目录、剪贴板、日志或未受硬件保护的 secret 文件。
- 供应链攻击者：可替换下载包、发布开发签名包、污染 Gradle/Rustup/依赖安装路径。

### 不覆盖范围

- 未攻击真实公网 relay。
- 未审计第三方 crate/AndroidX/OpenMLS/rustls 的源码实现。
- 未做数学级协议证明。
- 未进行移动端真机取证、越狱/root 场景测试。

## Findings

### SC-H-001 Android 发布 APK 使用 Debug 证书签名

等级：High
优先级：立即修复

影响：用户无法区分“项目 owner 发布的 APK”和“任意人用 debug keystore 重签的 APK”。后续升级也会绑定到 debug 证书，正式发布时可能无法平滑升级，且供应链攻击成本很低。

证据：

- `apps/android/SecureChatAndroid/app/build.gradle.kts:21-24` 的 release build 只设置 `isMinifyEnabled = false`，未配置 release signing。
- `script/build_android.sh:163-164` 仅执行 Gradle assemble task，没有签名策略检查。
- 本地验证命令输出：`apksigner verify --verbose --print-certs dist/SecureChatAndroid-0.2.4.apk` 通过了 APK Signature Scheme v2/v3，但证书 DN 是 `C=US, O=Android, CN=Android Debug`。
- 本地 artifact SHA-256：`52b5a5ab8cf747758b926619749f7a2ec965264578dc6c97456a72497ba9aa70  dist/SecureChatAndroid-0.2.4.apk`。

复现步骤：

```bash
apksigner verify --verbose --print-certs dist/SecureChatAndroid-0.2.4.apk
```

修复建议：

- 新建正式 release keystore，密钥不入库，使用 CI secret 或本机安全存储注入。
- 在 Gradle `release` build type 中配置 `signingConfig`，并开启 `v2SigningEnabled`、`v3SigningEnabled`。
- 发布脚本增加证书 subject/SHA-256 pin 检查，若证书是 `Android Debug` 则直接失败。
- 在 release notes 中公布 APK SHA-256 和签名证书 SHA-256。

复测方式：

```bash
./script/build_android.sh release
apksigner verify --verbose --print-certs path/to/release.apk
```

确认 signer 不再是 `Android Debug`，且证书 digest 与发布文档一致。

### SC-H-002 macOS 发布包仅 ad-hoc 签名且未 notarize

等级：High
优先级：立即修复

影响：macOS Gatekeeper 会拒绝应用，用户需要手动绕过系统保护；攻击者也可以构造外观相同的 ad-hoc signed app。对第一次使用的用户来说，这会把安全训练方向变成“忽略系统警告”。

证据：

- `script/package_macos.sh:78-80` 使用 `codesign --force --sign -` 签 dylib、binary 和 app bundle。
- `script/package_macos.sh:81` 只做 `codesign --verify --deep --strict`，没有 Developer ID、hardened runtime、notarization 或 staple。
- 本地验证：`codesign --verify --deep --strict dist/release/SecureChatMac.app` 通过，但 `spctl --assess --type execute --verbose=4 dist/release/SecureChatMac.app` 输出 `rejected`。
- 本地 DMG SHA-256：`a9c0580133d659f54b5cc249ac143b53671a92a780843ea63a67862f3c2d61fa  dist/SecureChatMac-0.2.4.dmg`。

复现步骤：

```bash
codesign --verify --deep --strict dist/release/SecureChatMac.app
spctl --assess --type execute --verbose=4 dist/release/SecureChatMac.app
```

修复建议：

- 使用 Developer ID Application 证书签名。
- 启用 hardened runtime，明确 entitlements。
- 对 DMG/app 做 notarization，完成后 staple。
- 发布脚本在 `spctl --assess` 不通过时失败。

复测方式：

```bash
codesign -dv --verbose=4 dist/release/SecureChatMac.app
spctl --assess --type execute --verbose=4 dist/release/SecureChatMac.app
```

期望 `spctl` 为 `accepted`，签名链显示 Developer ID。

### SC-H-003 Invite 元数据未认证，临时邀请语义可被改写

等级：High
优先级：立即修复

影响：攻击者只要能看到 invite URI，就可以重写 `mode`、`expires_unix` 或 `relay_hint`，而不破坏 bundle 签名。密钥材料本身仍会被 `DevicePreKeyBundle::verify()` 保护，但“临时邀请是否临时”“是否过期”“UI 展示的 relay hint”这些安全语义不可信。高风险场景中，临时邀请被改成永久联系人或延长有效期会破坏用户预期。

证据：

- `crates/secure-chat-core/src/invite.rs:7-15` 定义 `version/account_id/mode/relay_hint/expires_unix/bundle`。
- `crates/secure-chat-core/src/invite.rs:57-60` 只是把整个 invite JSON base64url 编入 `schat://invite/...`。
- `crates/secure-chat-core/src/invite.rs:73-78` 的 `verify()` 只检查版本、account ID 与 bundle，并调用 `bundle.verify()`，未校验 `mode`、`relay_hint`、`expires_unix` 的签名。
- `crates/secure-chat-desktop/src/lib.rs:1294-1329` 直接用 invite 中的 `relay_hint/expires_unix/mode` 构造预览。
- `crates/secure-chat-desktop/src/lib.rs:1332-1356` 用 invite 中的 `mode` 和 `expires_unix` 决定临时连接逻辑。

复现步骤：

1. 生成一个 `schat://invite/...`。
2. base64url 解码 payload，修改 JSON 中的 `mode` 或 `expires_unix`，再编码回 URI。
3. 调用客户端 preview/add temporary flow；只要 bundle 未变，`Invite::verify()` 不会因元数据改写失败。

修复建议：

- 用设备 signing key 对完整 invite payload 签名，或把 invite metadata 纳入现有签名 transcript。
- 将 `mode`、`expires_unix`、`relay_hint`、`version`、目标 bundle hash 放进被签名的 canonical payload。
- UI 明确显示 invite 签名状态；未签名旧格式只允许低风险兼容导入，并提示升级。

复测方式：

- 添加测试：修改 `mode`、`expires_unix`、`relay_hint` 任一字段后，`Invite::verify()` 必须失败。
- 手工复测旧 invite 兼容路径是否显示降级警告。

### SC-H-004 Relay 开放注册缺少 admission control，公开部署易被资源耗尽

等级：High
优先级：立即修复

影响：任何人都可以生成账号和设备身份并完成自签名注册。relay 只有全局设备数和单账号设备数上限，没有 IP、account 创建速率、proof-of-work、邀请码或管理员准入。公开 relay 可被大量注册、queue 填充、APNs token 写入、P2P candidate 发布等方式消耗 SQLite、内存和 CPU。

证据：

- `crates/secure-chat-relay/src/lib.rs:123-138` 暴露注册、消息、receipt、MLS、P2P、APNs endpoint。
- `crates/secure-chat-relay/src/lib.rs:501-524` 注册时验证的是 request bundle 自带设备签名；这能防止冒充某设备，但不限制任意新身份注册。
- `crates/secure-chat-relay/src/lib.rs:30-40` 定义了全局常量上限。
- `crates/secure-chat-relay/src/lib.rs:1198-1224` 只执行 `MAX_TOTAL_DEVICES=100000` 和 `MAX_DEVICES_PER_ACCOUNT=16`，没有网络来源或账号创建速率控制。
- `deploy/secure-chat-relay.service:15-27` 有 systemd sandboxing，这是积极控制，但不替代应用层 admission control。

复现步骤：

本地写一个循环生成 `DeviceKeyMaterial::generate(1)` 并调用 register endpoint。每个身份都可被视为合法新账号，直到 hit 全局 cap 或服务器资源瓶颈。

修复建议：

- 公共 relay 增加注册准入：邀请码、管理员审批、proof-of-work、per-IP/per-AS rate limit、注册冷却时间。
- 对 send/drain/claim/register 单独配置 `tower_governor` 或等价限流中间件。
- 将全局 cap、queue cap、payload cap、APNs token cap 配置化，并加 Prometheus/日志告警。
- 对 SQLite 写入做 backpressure，写失败或接近 cap 时优雅拒绝。

复测方式：

- 本地压测注册：超过阈值应返回 429，并且不增加 DB 行数。
- 模拟多 IP/单 IP 测试 rate limit 维度。

### SC-H-005 MLS KeyPackage claim 缺少关系授权，任意注册设备可消耗目标 KeyPackage

等级：High
优先级：立即修复

影响：`claim_mls_key_package` 验证了 requester 是已注册设备，但没有校验 requester 与 target 的关系、邀请、群成员资格或一次性领取 token。攻击者可以批量 claim 任意目标设备的 KeyPackage，造成群聊 onboarding DoS。KeyPackage 通常可公开，但“claim 并删除”是破坏性操作，需要授权。

证据：

- `crates/secure-chat-core/src/relay_api.rs:145-152` 的 claim request 只包含 requester 和 target ID，没有 group/invite/authorization token。
- `crates/secure-chat-relay/src/lib.rs:708-729` 只验证 requester 的签名。
- `crates/secure-chat-relay/src/lib.rs:730-738` 只检查 target device 存在。
- `crates/secure-chat-relay/src/lib.rs:738-749` 直接 `remove` target 的 KeyPackage 并删除 DB 记录。

复现步骤：

1. 注册攻击者设备 A 和目标设备 B。
2. B 发布 MLS KeyPackage。
3. A 使用自己的合法签名调用 `/v1/mls/key-packages/claim`，target 指向 B。
4. 返回 B 的 package，并使 B 的 package 从 relay 删除。

修复建议：

- 将 KeyPackage claim 绑定到被邀请者授权 token、联系人关系、群 invite 或 target 预授权。
- 或改为非破坏性 read，破坏性 consume 只允许 group creator 持有 target 生成的一次性 capability。
- 记录 claim 审计日志和 per-device claim rate limit。

复测方式：

- 添加测试：未建立关系的 requester claim target KeyPackage 必须返回 403。
- 添加测试：有效 invite/capability 才能 claim 且只能使用一次。

### SC-M-001 Relay auth nonce 重放缓存只在内存中，重启后存在短窗口重放风险

等级：Medium
优先级：下一版修复

影响：relay 请求签名绑定 action、request digest、timestamp、nonce，可以阻止在线重放。但 nonce cache 在内存中，relay 重启后 5 分钟 skew 窗口内捕获过的合法签名请求可能再次被接受。恶意 relay 或能观察 TLS 终端的攻击者可以制造重复 send、重复 claim、重复 APNs 注册/删除等副作用。

证据：

- `crates/secure-chat-core/src/relay_api.rs:262-315` 签名绑定 action、account/device、timestamp、nonce、request digest。
- `crates/secure-chat-core/src/relay_api.rs:283-299` 只检查 timestamp skew 和签名。
- `crates/secure-chat-relay/src/lib.rs:49-58` 的 `auth_nonces` 是内存 HashMap。
- `crates/secure-chat-relay/src/lib.rs:1183-1194` 只在内存中记录和裁剪 nonce。
- `crates/secure-chat-relay/src/lib.rs:1367-1428` 的 SQLite schema 没有 auth nonce 表。

复现步骤：

1. 捕获一个合法 signed request。
2. relay 处理一次后，立即重启 relay。
3. 在 `RELAY_AUTH_MAX_SKEW_SECS=300` 秒内重放同一 request。

修复建议：

- 持久化 `(device_id, nonce, issued_unix, action)`，至少保存 skew 窗口。
- 或使用 per-device 单调 counter，并持久化最后 counter。
- 对破坏性 endpoint 增加 idempotency key 或 message client ID，重复请求返回原结果而非重复副作用。

复测方式：

- 添加持久化 relay 测试：同一 signed request 在 restart 后必须返回 401/409。

### SC-M-002 Android 备份/迁移规则和 release hardening 不完整

等级：Medium
优先级：下一版修复

影响：当前核心数据目录使用 `noBackupFilesDir` 且 manifest 禁用了旧备份，方向正确；但 target Android 12+ 时 `allowBackup/fullBackupContent` 对 device-to-device transfer 不再完整覆盖。未来若新增 shared preferences、日志或缓存，可能被迁移。release 未开启 R8/minify，逆向成本较低。

证据：

- `apps/android/SecureChatAndroid/app/src/main/java/dev/securechat/android/core/SecureChatCoreClient.kt:8-11` 使用 `context.noBackupFilesDir`。
- `apps/android/SecureChatAndroid/app/src/main/AndroidManifest.xml:4-9` 设置 `allowBackup=false`、`fullBackupContent=false`，但未设置 `android:dataExtractionRules`。
- Android lint 报告 `apps/android/SecureChatAndroid/app/build/reports/lint-results-release.txt:33-51` 明确提示需要 data extraction rules。
- `apps/android/SecureChatAndroid/app/build.gradle.kts:21-24` release `isMinifyEnabled=false`。

复现步骤：

```bash
./dist/gradle/gradle-8.10.2/bin/gradle -p apps/android/SecureChatAndroid lintRelease
sed -n '33,51p' apps/android/SecureChatAndroid/app/build/reports/lint-results-release.txt
```

修复建议：

- 新增 `res/xml/data_extraction_rules.xml`，对 cloud backup 和 device transfer 都 exclude sensitive data。
- manifest 添加 `android:dataExtractionRules="@xml/data_extraction_rules"`。
- release 开启 R8/minify，并配置必要 keep rules。

复测方式：

- `lintRelease` 不再出现 `DataExtractionRules`。
- release APK 运行 smoke/unit/instrumented tests。

### SC-M-003 Android secret store 未使用硬件/系统 Keystore

等级：Medium
优先级：下一版修复

影响：Android 目标下，设备身份和 local storage key 存在 app-private no-backup 文件中，权限为 `0600`。这能防普通备份和其他 app 读取，但不具备 Android Keystore 的硬件绑定、StrongBox、用户认证策略或密钥不可导出能力。高风险场景中，root、取证镜像或 app sandbox 逃逸会直接暴露长期身份。

证据：

- `crates/secure-chat-desktop/src/lib.rs:2291-2300` 非 Android 使用 Keychain。
- `crates/secure-chat-desktop/src/lib.rs:2302-2329` Android 下 `load_secret/save_secret` 读写 secret 文件，创建权限为 `0o600`。
- `crates/secure-chat-desktop/src/lib.rs:2332-2338` secret 文件路径位于 data dir 下的 `secrets/<scope>/<kind>.secret`。
- `apps/android/SecureChatAndroid/app/src/main/java/dev/securechat/android/core/SecureChatCoreClient.kt:8-11` data dir 是 app no-backup 目录。

复现步骤：

在 Android debug/dev 环境查看 app 私有目录，会看到 secret 文件以 app sandbox 文件形式存在。

修复建议：

- 用 Android Keystore 生成不可导出 wrapping key。
- 用 Keystore wrapping key 加密 Rust runtime secrets，或通过 JNI 调用平台 secret provider。
- 支持 StrongBox/biometric/lockscreen-bound 选项，并在不可用时降级显示风险。

复测方式：

- 安装后检查 no-backup 目录中不再出现明文/可直接 base64 decode 的 long-term secret。
- 使用 Android Keystore attestation 或 `KeyInfo` 确认密钥不可导出。

### SC-M-004 群聊层不是完整 MLS 协议，安全声明需要降级或补齐

等级：Medium
优先级：发布前加固

影响：代码引入 OpenMLS ciphersuite label，但当前群聊实现是应用自管的 group epoch secret 和 Welcome control message，不是完整 RFC 9420 MLS state machine。群成员变更、admin 授权、commit/welcome 验证、移除成员后的一致性和 post-compromise security 不能按完整 MLS 宣称。

证据：

- `crates/secure-chat-core/src/group.rs:20-37` `GroupState/GroupWelcome` 直接包含 `secret` 和成员列表。
- `crates/secure-chat-core/src/group.rs:92-102` `from_welcome` 仅验证 protocol label 后接受 welcome 内容。
- `crates/secure-chat-core/src/group.rs:105-126` 添加成员后 rotate epoch 并返回 welcome。
- `crates/secure-chat-core/src/group.rs:150-185` 用当前 group secret 做 AEAD 加密。
- `README.md:9-11` 使用 “RFC 9420/OpenMLS-oriented” 表述是谨慎的；后续文档和产品文案应继续避免“完整 MLS”声明。

复现步骤：

静态确认 `GroupWelcome` 不是 OpenMLS Welcome 类型，且没有 MLS Commit 验证路径。

修复建议：

- 若短期保持现状，文档统一称为 “OpenMLS ciphersuite-oriented group prototype”。
- 若要支持高风险群聊，接入真实 OpenMLS group、credential、proposal/commit/welcome 流程。
- 对 group control message 增加 sender identity 签名和 group admin/member policy。

复测方式：

- 添加测试：非成员或被移除成员不能生成可被接受的 control message。
- 添加文档检查：release 文案不出现“完整 MLS 安全性”之类声明。

### SC-M-005 iOS/macOS 仍使用开发 bundle/signing 配置

等级：Medium
优先级：发布前加固

影响：bundle ID、Keychain service、APNs entitlement 和 signing team 是开发态配置。真实发布时会影响 Keychain 命名稳定性、APNs topic、证书权限和用户升级迁移；也容易让测试包和正式包混用身份。

证据：

- `script/package_macos.sh:5` macOS bundle ID 是 `dev.local.securechat.mac`。
- `crates/secure-chat-desktop/src/lib.rs:25-27` Keychain service 是 `dev.local.securechat`。
- `apps/ios/SecureChatIOS/SecureChatIOS/SecureChatIOS.entitlements:5-6` APNs 环境是 `development`。
- `apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj/project.pbxproj:381-382` 与 `:412-413` bundle ID 是 `dev.local.securechat.ios`。

复现步骤：

```bash
plutil -p apps/ios/SecureChatIOS/SecureChatIOS/SecureChatIOS.entitlements
grep -n "PRODUCT_BUNDLE_IDENTIFIER" apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj/project.pbxproj
```

修复建议：

- 确定正式 bundle ID，例如 `com.<org>.securechat.ios` 和 `com.<org>.securechat.mac`。
- 规划 Keychain service migration，不要无提示丢失现有身份。
- Release entitlements 使用 production APNs，Debug 保留 development。

复测方式：

- Xcode archive/export 后确认 entitlements、bundle ID、APNs topic 与 Apple Developer portal 一致。

### SC-M-006 CI 未覆盖安全发布门禁

等级：Medium
优先级：下一版修复

影响：当前 CI 只跑 Rust tests 和 macOS build；已发现的签名、lint、audit、artifact hash 问题不会自动阻断 release。后续修复容易回退。

证据：

- `.github/workflows/ci.yml:10-22` 只有 `cargo test` 和 `./script/build_and_run.sh --build-only`。
- 本地 `cargo audit --deny warnings` 通过，但 CI 未执行。
- Android `lintRelease testReleaseUnitTest` 本地通过但有 10 个 warning；CI 未执行。
- APK/DMG 签名状态本地可检测，但 release workflow 未强制。

复现步骤：

查看 `.github/workflows/ci.yml`。

修复建议：

- CI 增加 `cargo audit --deny warnings`、`cargo test --workspace`、Android `lintRelease testReleaseUnitTest`。
- release workflow 增加 `apksigner verify`、`codesign/spctl`、SHA-256 生成和校验。
- 对 release asset 的 hash 写入 `SHA256SUMS.txt` 并签名。

复测方式：

- 提交一个 debug-signed APK 或未 notarized app 的模拟 release，CI 必须失败。

### SC-L-001 部署脚本存在供应链 TOFU 风险

等级：Low
优先级：发布前加固

影响：一键部署脚本会通过 apt/snap/certbot/rustup 拉取工具链，方便但属于 trust-on-first-use。若包源、DNS、TLS、下载链路或本机 CA 被劫持，relay binary 构建环境可能被污染。

证据：

- `deploy/install-relay.sh:292-295` 安装 apt packages。
- `deploy/install-relay.sh:317-330` 可能通过 snap 安装/刷新 certbot。
- `deploy/install-relay.sh:351-354` 使用 `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y` 安装 Rust。
- `deploy/install-relay.sh:358-360` 服务器本机从源码构建 release relay。

复现步骤：

审阅 `deploy/install-relay.sh`。

修复建议：

- 为生产部署提供预构建、签名、校验和可复现构建说明。
- 若继续 server-side build，记录 `rustc --version`、`cargo --locked`、`Cargo.lock` hash。
- 对 Gradle distribution、Rustup installer、release binary 增加 checksum/pin。

复测方式：

- 离线或受控镜像环境部署，校验每个下载项 hash。

### SC-L-002 Android dependency 版本有更新提示

等级：Low
优先级：下一版修复

影响：lint 未报告直接漏洞，但多个 AndroidX/Compose/Test 依赖有新版本。长期滞后会错过安全修复和平台兼容修复。

证据：

- `apps/android/SecureChatAndroid/app/build/reports/lint-results-release.txt:1-24` 报告 Compose BOM、activity-compose、core-ktx、lifecycle、androidx.test 有新稳定版本。
- `apps/android/SecureChatAndroid/app/build.gradle.kts:48-64` 固定了这些依赖版本。

修复建议：

- 建立定期依赖升级节奏。
- Gradle dependency update/lint 进入 CI，但避免每次新版本都强制失败；安全相关依赖优先。

复测方式：

```bash
./dist/gradle/gradle-8.10.2/bin/gradle -p apps/android/SecureChatAndroid lintRelease testReleaseUnitTest
```

### SC-I-001 已确认的正向安全控制

等级：Info

已确认控制：

- 密钥生成使用 `OsRng`：`crates/secure-chat-core/src/identity.rs:75-116`、`crates/secure-chat-core/src/crypto.rs:50-54`。
- `DevicePreKeyBundle::verify()` 校验 account/device/signed-pre-key 签名：`crates/secure-chat-core/src/identity.rs:228-262`。
- X3DH-style session 把 bundle hash 纳入 initial transcript，并消费 one-time pre-key：`crates/secure-chat-core/src/protocol.rs:100-157`、`crates/secure-chat-core/src/protocol.rs:160-218`、`crates/secure-chat-core/src/identity.rs:214-216`。
- Message AEAD 使用 header/body associated data：`crates/secure-chat-core/src/protocol.rs:263-280`、`crates/secure-chat-core/src/protocol.rs:370-386`、`crates/secure-chat-core/src/protocol.rs:490-499`。
- Relay auth 签名绑定 action、request digest、timestamp、nonce：`crates/secure-chat-core/src/relay_api.rs:262-315`。
- Relay 拒绝 unsigned/replayed private command 的测试存在且本地通过：`crates/secure-chat-relay/src/lib.rs:1985-2028`。
- P2P candidate 读取要求已有 peer link：`crates/secure-chat-relay/src/lib.rs:821-827` 与测试 `crates/secure-chat-relay/src/lib.rs:2108-2139`。
- Receipt 越权有 grant 校验：`crates/secure-chat-relay/src/lib.rs:1245-1265` 与测试 `crates/secure-chat-relay/src/lib.rs:2141-2165`。
- 超大 ciphertext 会被拒绝：`crates/secure-chat-relay/src/lib.rs:895-903` 与测试 `crates/secure-chat-relay/src/lib.rs:2167-2182`。
- Android、macOS、iOS 剪贴板复制 invite/safety 信息后会尝试 120 秒过期或清空：`apps/android/SecureChatAndroid/app/src/main/java/dev/securechat/android/core/SecureClipboard.kt:14-24`、`apps/macos/SecureChatMac/Sources/SecureChatMac/Support/Clipboard.swift:3-13`、`apps/ios/SecureChatIOS/SecureChatIOS/Services/Clipboard.swift:3-10`。

## Fix Roadmap

### 立即修复

1. 配置 Android 正式 release signing，禁止 debug cert 发布。
2. 配置 macOS Developer ID + hardened runtime + notarization + staple。
3. 对完整 invite payload 加签，拒绝被篡改的 `mode/expires/relay_hint`。
4. Relay 增加注册准入、限流和公开服务资源阈值配置。
5. MLS KeyPackage claim 增加关系授权或一次性 capability。

### 下一版修复

1. 持久化 relay auth nonce 或引入 per-device counter。
2. Android 添加 `dataExtractionRules`，启用 release minify/R8。
3. Android secret store 接入 Keystore wrapping。
4. CI 增加 `cargo audit`、Android lint/unit 和 release 签名检查。
5. 升级 AndroidX/Compose 依赖并跑回归测试。

### 发布前加固

1. 统一正式 bundle ID、Keychain service、APNs production entitlement。
2. 生产部署提供签名二进制和 checksum，而不是只提供 server-side `curl|sh` 构建路径。
3. 将 release artifact hash 和签名证书 hash 写入 release notes。
4. 明确文档安全声明：当前是原型，不是第三方审计后的高风险通信工具。

### 长期研究

1. 用完整 OpenMLS 替换当前 group prototype。
2. 对 Double Ratchet/X3DH-style 协议做第三方密码学审计或形式化模型验证。
3. 增加多设备账号模型、设备撤销、密钥轮换和安全码变更 UX。
4. 增加恶意 relay 模拟器和移动端真机/模拟器安全回归套件。

## Appendix

### 运行过的命令与结果

```bash
git status -sb
git log --oneline --decorate -n 1
```

结果：当前 head 为 `4b946da (HEAD -> main, tag: v0.2.4, origin/main) Fix Android one-to-one chat routing`。工作区已有未提交改动：`README.md` 修改，以及 `docs/zh/client-installation.md` 未跟踪。本报告只新增审计文件。

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace -- --nocapture
```

结果：全部通过。`secure-chat-client` 5 passed，`secure-chat-core` 16 passed，`secure-chat-desktop` 5 passed，`secure-chat-relay` 8 passed，FFI/doc tests 无失败。

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo audit --deny warnings
```

结果：退出码 0，未报告 RustSec advisory。

```bash
ANDROID_HOME=/opt/homebrew/share/android-commandlinetools \
ANDROID_SDK_ROOT=/opt/homebrew/share/android-commandlinetools \
./dist/gradle/gradle-8.10.2/bin/gradle -p apps/android/SecureChatAndroid lintRelease testReleaseUnitTest
```

结果：BUILD SUCCESSFUL，0 errors，10 warnings。主要 warning 是 `DataExtractionRules`、`MissingApplicationIcon` 和依赖有更新。

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
SECURE_CHAT_SMOKE_MODE=group PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
SECURE_CHAT_SMOKE_MODE=p2p PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
```

结果：三项均返回 `"ok": true`。一对一 relay smoke、group/APNs-token smoke、P2P rendezvous/direct datagram smoke 均通过。

```bash
apksigner verify --verbose --print-certs dist/SecureChatAndroid-0.2.4.apk
codesign --verify --deep --strict dist/release/SecureChatMac.app
spctl --assess --type execute --verbose=4 dist/release/SecureChatMac.app
shasum -a 256 dist/SecureChatMac-0.2.4.dmg dist/SecureChatAndroid-0.2.4.apk dist/SHA256SUMS.txt
```

结果：APK v2/v3 signature 验证通过但 signer 为 Android Debug；macOS codesign 验证通过但 `spctl` rejected；hash 如下：

```text
a9c0580133d659f54b5cc249ac143b53671a92a780843ea63a67862f3c2d61fa  dist/SecureChatMac-0.2.4.dmg
52b5a5ab8cf747758b926619749f7a2ec965264578dc6c97456a72497ba9aa70  dist/SecureChatAndroid-0.2.4.apk
959b53ca3f6feb807c6576bdf8802b4d8c80e50405df9c9dba9c7a55920c44f2  dist/SHA256SUMS.txt
```

### 检查过的危险模式

使用 `rg` 定向检查了：

- `unwrap`、`expect`、`panic!`
- 明文 secret/token/password/private key
- 未鉴权 endpoint
- `http://` fallback
- `curl | sh`、`sudo`、`ufw`、文件权限
- `codesign`、`apksigner`、debug signing
- Android backup/data extraction/minify
- tracing/logging/APNs token

结论：测试代码中有大量 `unwrap` 是正常；生产入口 `crates/secure-chat-relay/src/main.rs` 对错误配置使用 `expect/panic`，属于可用性风险但不直接泄露 secret。未发现硬编码真实 APNs 私钥、密码或 token。
