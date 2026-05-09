# SecureChat Windows 客户端说明

Windows 客户端从 v0.2.6 开始加入，目标是与 macOS、iOS、Android 的 v0.2.5
聊天能力对齐。它使用 WinUI 3、Windows App SDK、C# P/Invoke 和
`secure_chat_ffi.dll`，协议、加密、附件分片、阅后即焚和本地数据库逻辑仍由 Rust
runtime 处理。

## 系统要求

- Windows 10 22H2 或更新版本。
- 推荐 Windows 11。
- 构建机需要安装 .NET 8 SDK、Windows App SDK/WinUI 构建工具、Visual Studio
  2022 或 Build Tools，以及 Rust MSVC 工具链。

## 构建

在 Windows PowerShell 中从仓库根目录运行：

```powershell
.\script\build_windows.ps1 -Configuration Release
```

脚本会执行：

1. 安装 Rust `x86_64-pc-windows-msvc` target。
2. 构建 `secure_chat_ffi.dll`。
3. 把 DLL 复制到 `apps/windows/SecureChatWindows/Native`。
4. 生成或复用本地测试签名证书。
5. 构建 WinUI 3 app 并生成签名 MSIX。
6. 复制 `dist/SecureChatWindows-0.2.6.msix` 并写出 SHA-256。

正式公开分发时，请用可信代码签名证书替换测试证书，并在 Release notes 中公布
证书指纹和 SHA-256。

## 功能范围

Windows 端支持：

- 创建或恢复本地身份，设置 Relay URL。
- 复制普通邀请和临时邀请，导入联系人邀请。
- 一对一聊天、群聊和临时会话。
- 联系人昵称编辑、本地强删除、群成员添加、临时会话结束。
- 文本和 Unicode emoji。
- 文件、图片、本地表情图片/GIF 导入与发送。
- 图片和表情缩略图，附件打开。
- 阅后即焚消息打开后本地销毁，并发送销毁通知。
- 自动轮询接收、手动 Receive、系统通知。
- 聊天列表智能滚动：贴底才自动跟随，回看历史时显示 New messages 按钮。

## 本地安全

- UI 不直接处理私钥材料，只通过 Rust FFI 调用 runtime。
- 设备身份密钥和本地 storage key 存入 Windows Credential Manager/DPAPI。
- SQLite、附件、表情和临时文件保存在 `%LOCALAPPDATA%\SecureChat`。
- 消息正文和 session state 继续由 Rust runtime 用 storage key 加密后落盘。
- 错误提示只展示脱敏后的 runtime 错误，不写明文消息日志。

## 安装测试证书

如果使用 `build_windows.ps1` 自动生成的测试证书，安装 MSIX 前需要信任证书：

1. 打开 `dist/SecureChatWindows-TemporaryKey.cer`。
2. 选择 `Install Certificate`。
3. 选择 `Current User`。
4. 选择 `Place all certificates in the following store`。
5. 选择 `Trusted People`。
6. 完成后双击 `dist/SecureChatWindows-0.2.6.msix` 安装。

测试证书只适合本地或内部分发。正式发布必须使用可信代码签名证书。

## 验证清单

- 空状态启动并创建身份。
- Relay URL 更新后设备重新注册。
- 与 macOS/iOS/Android 分别互加联系人。
- 一对一、群聊、临时会话收发文本、emoji、图片、文件、表情和阅后即焚消息。
- 回看历史时收到新消息不跳到底部；点击 New messages 后回到底部。
- 用户自己发送消息后立即滚到最新。
- 删除联系人后本地联系人、1:1 历史、session 和未完成附件被清理。
- 打开阅后即焚消息后内容隐藏，相关附件文件被清理。
- MSIX 安装、覆盖升级、卸载、重新安装流程正常。

