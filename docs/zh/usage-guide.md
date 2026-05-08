# SecureChat 使用教程

这份教程按“两个人通过公共 relay 互聊”的流程写。假设服务器已经部署好，域名是：

```text
chat.example.com
```

你可以把下面所有 `chat.example.com` 替换成自己的 relay 域名。

## 你需要准备什么

- 两台 macOS 电脑，或同一台电脑运行两个不同数据目录的客户端
- 一个可访问的 relay：
  - QUIC URL：`quic://chat.example.com:443`
  - HTTPS URL：`https://chat.example.com`
- 一个可信的带外渠道用来比对安全码，例如当面扫码、电话读码、可信视频通话

## 启动客户端

在项目目录构建并打开 macOS App：

```bash
./script/build_and_run.sh --verify
```

如果只想构建不启动：

```bash
./script/build_and_run.sh --build-only
```

打开后第一次会看到登录界面。

## 第一次创建本地身份

在登录界面填写：

- `Display name`：本地显示名，例如 `Alice`
- `Relay URL`：推荐填 `quic://chat.example.com:443`

点击 `Create / Login`。

客户端会在本机创建匿名账号和设备身份：

- 身份私钥保存到 macOS Keychain
- 联系人、会话状态和本地消息缓存保存到 SQLite
- 本地消息正文和 ratchet session 会加密后落盘

同一个数据目录再次打开时，会继续使用同一套身份。

## 添加好友

### 1. Alice 复制邀请

Alice 点击工具栏里的 `Invite`，再点 `Copy`。

复制出来的是一段：

```text
schat://invite/...
```

这个邀请包含 Alice 的匿名账号 ID、设备公钥、pre-key bundle、安全指纹素材和 relay hint。它不包含 Alice 的私钥。

### 2. Alice 通过可信渠道发给 Bob

可以先用微信、邮件、AirDrop、短信或当面传递。注意：这些渠道只用于传递邀请，不承担消息加密。

### 3. Bob 导入邀请

Bob 点击工具栏里的 `Add Contact`：

- `Display name`：填 `Alice`
- `schat://invite/...`：粘贴 Alice 的邀请

点击 `Add`。

Bob 的联系人列表会出现 Alice。

### 4. Bob 也把自己的邀请发给 Alice

为了让双方都能主动发起会话，Bob 也点击 `Invite` -> `Copy`，把自己的邀请发给 Alice。Alice 用 `Add Contact` 导入 Bob 的邀请。

## 比对安全码

点开联系人后，聊天顶部会显示：

```text
Safety 123456...
```

上线前应该通过带外渠道比对安全码：

- 当面看双方屏幕
- 电话读出数字
- 视频通话确认
- 后续可扩展成二维码扫码确认

如果安全码变化，可能是对方换设备、重新安装、设备列表变化，也可能是中间人攻击。生产使用时不要静默忽略安全码变化。

## 发送消息

选择联系人，在底部输入框输入消息，点击 `Send`，或使用：

```text
Command + Return
```

消息状态会显示在气泡下方：

- `sent`：本机已加密并提交到 relay
- `delivered`：对方设备已从 relay 拉取密文
- `read`：对方客户端已解密并发送已读回执
- `received`：本机收到的入站消息
- `failed`：发送失败或本地处理失败

## 接收消息

工具栏有两个接收相关控件：

- `Receive`：手动从 relay 拉取密文和回执
- `Auto`：后台自动轮询，默认开启

收到新消息时，客户端会触发 macOS 通知。第一次使用时，系统可能会询问是否允许 SecureChat 发送通知。

## 切换 relay

点击工具栏里的 `Relay`：

- QUIC 优先：`quic://chat.example.com:443`
- HTTPS 兜底：`https://chat.example.com`
- 本地开发：`http://127.0.0.1:8787`

保存后客户端会重新注册当前设备的 public pre-key bundle。

## 两个人跨公网互聊的最短路径

1. 服务器部署 relay。
2. Alice 和 Bob 都把 Relay URL 设置成 `quic://chat.example.com:443`。
3. Alice 复制 invite 给 Bob。
4. Bob 导入 Alice invite。
5. Bob 复制 invite 给 Alice。
6. Alice 导入 Bob invite。
7. 双方比对 Safety code。
8. 开始发送消息。

## 同一台 Mac 上测试两个账号

默认 app 使用固定数据目录。想在同一台机器上模拟两个人，可以用两个不同数据目录直接调用 Rust smoke：

```bash
cargo run -p secure-chat-client --bin secure-chat-smoke
```

这个 smoke 会自动创建 Alice 和 Bob，注册设备，发送 E2EE 消息，收取并解密，然后输出 JSON 报告。

如果要跑两个 GUI 实例，后续可以给 Swift app 增加数据目录启动参数。当前 GUI 原型主要面向一台机器一个本地身份。

## 本地数据在哪里

桌面 runtime 使用：

- Keychain：设备身份私钥和本地存储密钥
- SQLite：profile、contacts、encrypted sessions、encrypted messages、relay ciphertext cache

删除 app bundle 不会自动删除 Keychain 身份。重装后如果数据目录和 Keychain scope 一致，会继续使用原身份。

## 常见问题

### 登录时 relay 连接失败

先确认 relay 健康：

```bash
curl -fsS https://chat.example.com/health
```

如果 HTTPS 可用但 QUIC 不可用，客户端 Relay URL 改成：

```text
https://chat.example.com
```

### 发出去一直是 sent

说明消息已提交到 relay，但对方还没有拉取。让对方确认：

- app 正在运行
- `Auto` 开启，或手动点 `Receive`
- relay URL 和你使用的是同一台服务器

### delivered 有了但没有 read

说明对方设备已经拉取密文，但还没有成功解密/渲染/发送已读回执。检查对方 app 是否打开对应联系人，或是否有错误提示。

### Add Contact 失败

检查邀请是否完整，必须以：

```text
schat://invite/
```

开头。复制时不要丢掉末尾字符。

### 安全码变化

常见原因：

- 对方重新安装并生成了新身份
- 对方新增或移除设备
- 你导入了一个新的 invite

处理方式：

1. 暂停发送敏感消息。
2. 通过带外渠道确认对方身份。
3. 确认后重新标记为可信。

## 当前限制

- 只支持 1 对 1 聊天。
- 没有手机号、邮箱和公开用户名搜索。
- macOS app 使用后台轮询，不是 APNs 推送。
- P2P NAT traversal 还没有完整实现，当前生产路径以 relay 为主。
- 没有第三方安全审计前，不要把它当成已经审计过的安全通信产品。
