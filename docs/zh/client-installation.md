# SecureChat 客户端安装与首次使用说明

这份说明写给第一次使用 SecureChat 的用户。你只需要完成三件事：

1. 安装客户端。
2. 填写自己的显示名和 Relay URL。
3. 和好友互换邀请链接后开始聊天。

SecureChat 的消息内容是端到端加密的。relay 服务器只负责转发密文、离线队列和回执，不保存聊天明文。

## 安装前准备

向服务提供者或群管理员确认一个 Relay URL。常见格式是：

```text
quic://chat.example.com:443
https://chat.example.com
```

推荐优先使用 `quic://...:443`。如果当前网络阻断 UDP 或 QUIC，再切换到 `https://...`。

客户端下载地址：

```text
https://github.com/ninjaz0/secure-chat/releases/latest
```

只从项目的 GitHub Releases 下载客户端，不要安装别人转发的来历不明安装包。

## macOS 客户端安装

适用于 Mac 用户。

1. 打开最新版 Release 页面。
2. 下载 `SecureChatMac-版本号.dmg`，例如 `SecureChatMac-0.2.5.dmg`。
3. 双击打开 DMG。
4. 把 `SecureChatMac.app` 拖到 `Applications` / `应用程序` 文件夹。
5. 打开 `应用程序`，启动 `SecureChatMac`。

如果 macOS 提示“无法打开，因为无法验证开发者”：

1. 在 Finder 里打开 `应用程序`。
2. 按住 `Control`，点击 `SecureChatMac`。
3. 选择 `打开`。
4. 在弹窗里再次点 `打开`。

这是因为当前测试版使用本地签名，还没有 Apple notarization。只要安装包来自项目 GitHub Release，就可以按上面的方式打开。

## Android 客户端安装

适用于 Android 手机或平板。

1. 打开最新版 Release 页面。
2. 下载 `SecureChatAndroid-版本号.apk`，例如 `SecureChatAndroid-0.2.5.apk`。
3. 在手机上打开下载完成的 APK。
4. 如果系统提示“禁止安装未知应用”，进入提示页，允许当前浏览器或文件管理器安装未知应用。
5. 回到安装界面，点击 `安装`。
6. 安装完成后，打开 `SecureChat`。

安装完成后，建议关闭“允许安装未知应用”权限，避免其他 APK 被误装。

## iOS 客户端安装

iOS 目前不是 App Store 公开上架版本。可用方式有两种：

- 如果你收到了 TestFlight 邀请：先安装 Apple 的 TestFlight App，再通过邀请链接安装 SecureChat。
- 如果你是测试用户或开发者：需要使用 Xcode 安装到 iPhone 或 iPad。

使用 Xcode 安装的步骤请参考：

```text
docs/zh/ios-client.md
```

简要流程是：

1. 在 Mac 上安装 Xcode 和 Rust。
2. 在项目目录运行 `./script/build_ios.sh debug`。
3. 打开 `apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj`。
4. 在 Xcode 里设置自己的 Apple Development Team。
5. 连接 iPhone，选择真机，点击 Run。
6. 如果手机提示“不受信任的开发者”，到 `设置 -> 通用 -> VPN 与设备管理` 信任证书。

## 第一次打开客户端

第一次打开时会看到登录或初始化界面。

填写：

- `Display name`：你的显示名，例如 `Alice` 或 `小王`。
- `Relay URL`：服务提供者给你的地址，例如 `quic://chat.example.com:443`。

然后点击 `Start`、`Create / Login` 或类似按钮。

客户端会在本机创建匿名账号和设备身份。请注意：

- 显示名只影响本地展示，不等于账号密码。
- 不需要手机号、邮箱或注册密码。
- 删除 App 或清空数据后，本机身份可能会变化，需要重新添加好友。

## 添加好友并开始一对一聊天

假设 Alice 要添加 Bob：

1. Bob 打开客户端，进入邀请页，点击 `Invite` 或 `Copy Invite`。
2. Bob 把复制出来的 `schat://invite/...` 发给 Alice。
3. Alice 打开 `Add Contact`。
4. 粘贴 Bob 的邀请链接。
5. 点击 `Check` 或预览按钮。
6. 确认显示的是 Bob 的设备信息后，填写备注名并点击 `Add`。
7. Alice 进入 Bob 的聊天窗口，发送第一条消息。
8. Bob 打开客户端后会自动收取；也可以点击刷新或 Receive 按钮手动收取。

为了确认没有加错人，建议双方通过电话、当面或可信视频通话比对安全码。安全码一致后，说明双方当前设备身份匹配。

## 发送更多类型的消息

v0.2.5 起，macOS、iOS 和 Android 的聊天页都支持：

- 文本和 Unicode emoji。
- 图片消息和普通文件消息。
- 本地导入表情图片/GIF，并在聊天中作为表情发送。
- 阅后即焚消息：打开后本机立即隐藏内容，并尽力向对方发送销毁通知。

如果你正在聊天底部，新消息会自动滚到最新；如果你正在回看历史，新消息不会打断当前位置，会出现一个可点击的新消息提示。

## 联系人昵称和删除

联系人详情或更多菜单里可以编辑昵称，也可以删除联系人。删除是本地强删除，会清理该联系人、1 对 1 消息、会话密钥和相关未完成附件；不会删除对方设备上的历史。

## 临时聊天

临时聊天适合短时间交流，不想保存为长期联系人时使用。

1. 一方点击 `Temporary` 或 `Copy Temporary` 生成临时邀请。
2. 另一方粘贴临时邀请并开始临时会话。
3. 临时邀请和临时消息有有效期。
4. 结束后可点击 `End` 删除临时会话。

如果你想长期联系对方，请使用普通 Invite 添加为联系人。

## 更换 Relay URL

如果管理员换了服务器地址：

1. 打开客户端设置页。
2. 找到 `Relay URL`。
3. 填入新的地址。
4. 点击 `Update Relay`。
5. 返回聊天页，点一次刷新或 Receive。

两个人必须连接到同一个 relay，才能通过离线队列互相收发消息。

## 更新客户端

有新版时：

- macOS：下载新的 DMG，把新的 `SecureChatMac.app` 拖进 `应用程序` 并替换旧版。
- Android：下载新的 APK，直接安装覆盖旧版。
- iOS：通过 TestFlight 更新，或用 Xcode 重新 Run 一次。

正常覆盖安装不会删除本地聊天数据。为了稳妥，更新前先退出正在运行的旧客户端。

## 常见问题

### 收不到消息

按顺序检查：

1. 双方 Relay URL 是否完全一致。
2. 手机或电脑是否能访问公网。
3. 对方是否已经发送消息。
4. 是否点击过刷新或 Receive。
5. 如果 `quic://` 不通，改用 `https://` 试一次。

### 邀请链接无法识别

确认邀请以 `schat://invite/` 开头。不要手动改动邀请内容，也不要只复制一部分。

### Android 安装失败

确认下载的是 `.apk` 文件，并且 Android 系统允许当前浏览器或文件管理器安装未知应用。安装完成后可以关闭该权限。

### macOS 打不开

优先用 `Control` + 点击 App，再选择 `打开`。如果仍然打不开，删除旧版 App 后重新从 Release 页面下载 DMG。

### iOS 无法安装到真机

检查 Xcode 的 Team、Bundle Identifier 和手机开发者模式。免费 Apple ID 的签名可能会过期，过期后用 Xcode 重新安装。

## 安全提醒

- 不要把自己的设备或系统密码发给任何人。
- 邀请链接可以用普通聊天工具转发，但安全码最好通过可信渠道比对。
- 如果发现对方安全码变化，先暂停发送敏感内容，确认对方是否换了设备或重新安装了客户端。
- SecureChat 仍是原型级软件，适合测试和自托管试用，不应在未审计前用于高风险通信场景。
