use secure_chat_core::{
    accept_session_as_responder, safety_number, start_session_as_initiator, CipherSuite,
    DeviceKeyMaterial, Invite, ObfuscationProfile, PlainMessage, TransportFrame,
};
use serde::Serialize;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(Serialize)]
struct DemoState {
    ok: bool,
    account_id: String,
    device_id: String,
    invite_uri: String,
    safety_number: String,
    safety_qr_payload: String,
    relay_hint: String,
    protocol_checks: Vec<ProtocolCheck>,
    contacts: Vec<ContactPreview>,
    messages: Vec<MessagePreview>,
}

#[derive(Serialize)]
struct ProtocolCheck {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
struct ContactPreview {
    display_name: String,
    account_id: String,
    device_id: String,
    verified: bool,
}

#[derive(Serialize)]
struct MessagePreview {
    sender: String,
    body: String,
    encrypted_bytes: usize,
}

#[no_mangle]
pub extern "C" fn secure_chat_demo_state_json() -> *mut c_char {
    json_to_c_string(build_demo_state())
}

#[no_mangle]
pub extern "C" fn secure_chat_protocol_self_test_json() -> *mut c_char {
    let checks = run_protocol_checks();
    json_to_c_string(serde_json::json!({
        "ok": checks.iter().all(|check| check.passed),
        "checks": checks,
    }))
}

#[no_mangle]
pub extern "C" fn secure_chat_relay_smoke_json() -> *mut c_char {
    let value = match tokio::runtime::Runtime::new() {
        Ok(runtime) => match runtime.block_on(secure_chat_client::run_relay_smoke()) {
            Ok(report) => {
                serde_json::to_value(report).unwrap_or_else(|err| error_json(err.to_string()))
            }
            Err(err) => error_json(err.to_string()),
        },
        Err(err) => error_json(err.to_string()),
    };
    json_to_c_string(value)
}

#[no_mangle]
pub extern "C" fn secure_chat_p2p_smoke_json() -> *mut c_char {
    let value = match tokio::runtime::Runtime::new() {
        Ok(runtime) => match runtime.block_on(secure_chat_client::run_p2p_smoke()) {
            Ok(report) => {
                serde_json::to_value(report).unwrap_or_else(|err| error_json(err.to_string()))
            }
            Err(err) => error_json(err.to_string()),
        },
        Err(err) => error_json(err.to_string()),
    };
    json_to_c_string(value)
}

#[no_mangle]
pub extern "C" fn secure_chat_app_snapshot_json(data_dir: *const c_char) -> *mut c_char {
    json_to_c_string(match c_arg(data_dir, "data_dir") {
        Ok(data_dir) => match secure_chat_desktop::DesktopRuntime::open(data_dir)
            .and_then(|runtime| runtime.snapshot())
        {
            Ok(snapshot) => to_value(snapshot),
            Err(err) => error_json(err.to_string()),
        },
        Err(err) => error_json(err),
    })
}

#[no_mangle]
pub extern "C" fn secure_chat_app_bootstrap_json(
    data_dir: *const c_char,
    display_name: *const c_char,
    relay_url: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(display_name, "display_name"),
            c_arg(relay_url, "relay_url"),
        ) {
            (Ok(data_dir), Ok(display_name), Ok(relay_url)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::bootstrap(
                        data_dir,
                        &display_name,
                        &relay_url,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, display_name, relay_url) => error_json(
                data_dir
                    .err()
                    .or_else(|| display_name.err())
                    .or_else(|| relay_url.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_update_relay_json(
    data_dir: *const c_char,
    relay_url: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (c_arg(data_dir, "data_dir"), c_arg(relay_url, "relay_url")) {
            (Ok(data_dir), Ok(relay_url)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::update_relay(
                        data_dir, &relay_url,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, relay_url) => error_json(
                data_dir
                    .err()
                    .or_else(|| relay_url.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_invite_json(data_dir: *const c_char) -> *mut c_char {
    json_to_c_string(match c_arg(data_dir, "data_dir") {
        Ok(data_dir) => match secure_chat_desktop::DesktopRuntime::invite(data_dir) {
            Ok(invite) => to_value(invite),
            Err(err) => error_json(err.to_string()),
        },
        Err(err) => error_json(err),
    })
}

#[no_mangle]
pub extern "C" fn secure_chat_app_preview_invite_json(
    data_dir: *const c_char,
    invite_text: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(invite_text, "invite_text"),
        ) {
            (Ok(data_dir), Ok(invite_text)) => {
                match secure_chat_desktop::DesktopRuntime::preview_invite(data_dir, &invite_text) {
                    Ok(preview) => to_value(preview),
                    Err(err) => error_json(err.to_string()),
                }
            }
            (data_dir, invite_text) => error_json(
                data_dir
                    .err()
                    .or_else(|| invite_text.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_add_contact_json(
    data_dir: *const c_char,
    display_name: *const c_char,
    invite_uri: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(display_name, "display_name"),
            c_arg(invite_uri, "invite_uri"),
        ) {
            (Ok(data_dir), Ok(display_name), Ok(invite_uri)) => {
                match secure_chat_desktop::DesktopRuntime::add_contact(
                    data_dir,
                    &display_name,
                    &invite_uri,
                ) {
                    Ok(snapshot) => to_value(snapshot),
                    Err(err) => error_json(err.to_string()),
                }
            }
            (data_dir, display_name, invite_uri) => error_json(
                data_dir
                    .err()
                    .or_else(|| display_name.err())
                    .or_else(|| invite_uri.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_send_message_json(
    data_dir: *const c_char,
    contact_id: *const c_char,
    body: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(contact_id, "contact_id"),
            c_arg(body, "body"),
        ) {
            (Ok(data_dir), Ok(contact_id), Ok(body)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::send_message(
                        data_dir,
                        &contact_id,
                        &body,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, contact_id, body) => error_json(
                data_dir
                    .err()
                    .or_else(|| contact_id.err())
                    .or_else(|| body.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_receive_json(data_dir: *const c_char) -> *mut c_char {
    json_to_c_string(match c_arg(data_dir, "data_dir") {
        Ok(data_dir) => desktop_async(|runtime| {
            runtime
                .block_on(secure_chat_desktop::DesktopRuntime::receive(data_dir))
                .map(to_value)
                .unwrap_or_else(|err| error_json(err.to_string()))
        }),
        Err(err) => error_json(err),
    })
}

#[no_mangle]
pub extern "C" fn secure_chat_app_p2p_probe_json(data_dir: *const c_char) -> *mut c_char {
    json_to_c_string(match c_arg(data_dir, "data_dir") {
        Ok(data_dir) => desktop_async(|runtime| {
            runtime
                .block_on(secure_chat_desktop::DesktopRuntime::p2p_probe(data_dir))
                .map(to_value)
                .unwrap_or_else(|err| error_json(err.to_string()))
        }),
        Err(err) => error_json(err),
    })
}

#[no_mangle]
pub extern "C" fn secure_chat_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

fn desktop_async(
    work: impl FnOnce(tokio::runtime::Runtime) -> serde_json::Value,
) -> serde_json::Value {
    match tokio::runtime::Runtime::new() {
        Ok(runtime) => work(runtime),
        Err(err) => error_json(err.to_string()),
    }
}

fn c_arg(ptr: *const c_char, name: &str) -> Result<String, String> {
    if ptr.is_null() {
        return Err(format!("{name} is null"));
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|value| value.to_string())
        .map_err(|err| format!("{name} is not utf-8: {err}"))
}

fn to_value(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|err| error_json(err.to_string()))
}

fn build_demo_state() -> serde_json::Value {
    match try_build_demo_state() {
        Ok(state) => serde_json::to_value(state).unwrap_or_else(|err| error_json(err.to_string())),
        Err(err) => error_json(err),
    }
}

fn try_build_demo_state() -> Result<DemoState, String> {
    let alice = DeviceKeyMaterial::generate(8);
    let bob = DeviceKeyMaterial::generate(8);
    let bob_bundle = bob.pre_key_bundle();
    let alice_invite = Invite::new(
        alice.pre_key_bundle(),
        Some("https://relay.local/v1".to_string()),
        Some(1_900_000_000),
    )
    .to_uri()
    .map_err(|err| err.to_string())?;
    let (initial, mut alice_session) =
        start_session_as_initiator(&alice, &bob_bundle, CipherSuite::ChaCha20Poly1305)
            .map_err(|err| err.to_string())?;
    let mut bob_session =
        accept_session_as_responder(&bob, &initial).map_err(|err| err.to_string())?;
    let safety = safety_number(&[alice.public_identity()], &[bob.public_identity()]);
    alice_session.mark_verified();
    bob_session.mark_verified();

    let wire = alice_session
        .encrypt(PlainMessage {
            sent_at_unix: 1_900_000_001,
            body: "这是一条端到端加密的本地演示消息。".to_string(),
        })
        .map_err(|err| err.to_string())?;
    let encrypted_bytes = serde_json::to_vec(&wire)
        .map_err(|err| err.to_string())?
        .len();
    let opened = bob_session.decrypt(&wire).map_err(|err| err.to_string())?;
    let frame = TransportFrame::protect(
        &serde_json::to_vec(&wire).map_err(|err| err.to_string())?,
        &ObfuscationProfile::stealth_quic(),
    )
    .map_err(|err| err.to_string())?;

    Ok(DemoState {
        ok: true,
        account_id: alice.account_id.to_string(),
        device_id: alice.device_id.to_string(),
        invite_uri: alice_invite,
        safety_number: safety.number,
        safety_qr_payload: safety.qr_payload,
        relay_hint: "P2P QUIC 优先，WebSocket/TLS 与 relay 自动兜底".to_string(),
        protocol_checks: vec![
            ProtocolCheck {
                name: "X3DH 风格握手".to_string(),
                passed: true,
                detail: "双方从 identity key、signed pre-key、one-time pre-key 派生相同初始密钥"
                    .to_string(),
            },
            ProtocolCheck {
                name: "双棘轮消息加密".to_string(),
                passed: opened.body.contains("端到端加密"),
                detail: "每条消息派生一次性 body/header key，解密后推进链状态".to_string(),
            },
            ProtocolCheck {
                name: "流量混淆帧".to_string(),
                passed: frame.padded_body.len() == 1200,
                detail: "密文封装为固定 1200 字节 QUIC-like frame，并保留 fallback 传输抽象"
                    .to_string(),
            },
        ],
        contacts: vec![ContactPreview {
            display_name: "Bob 的匿名设备".to_string(),
            account_id: bob.account_id.to_string(),
            device_id: bob.device_id.to_string(),
            verified: true,
        }],
        messages: vec![MessagePreview {
            sender: "Alice".to_string(),
            body: opened.body,
            encrypted_bytes,
        }],
    })
}

fn run_protocol_checks() -> Vec<ProtocolCheck> {
    let mut checks = Vec::new();
    let alice = DeviceKeyMaterial::generate(4);
    let bob = DeviceKeyMaterial::generate(4);
    let (initial, mut alice_session) = match start_session_as_initiator(
        &alice,
        &bob.pre_key_bundle(),
        CipherSuite::ChaCha20Poly1305,
    ) {
        Ok(result) => result,
        Err(err) => {
            checks.push(failed("X3DH 风格握手", err.to_string()));
            return checks;
        }
    };
    let mut bob_session = match accept_session_as_responder(&bob, &initial) {
        Ok(session) => {
            checks.push(passed("X3DH 风格握手", "初始会话建立成功"));
            session
        }
        Err(err) => {
            checks.push(failed("X3DH 风格握手", err.to_string()));
            return checks;
        }
    };
    let wire = match alice_session.encrypt(PlainMessage {
        sent_at_unix: 42,
        body: "self-test".to_string(),
    }) {
        Ok(wire) => wire,
        Err(err) => {
            checks.push(failed("双棘轮加密", err.to_string()));
            return checks;
        }
    };
    match bob_session.decrypt(&wire) {
        Ok(plain) if plain.body == "self-test" => checks.push(passed("双棘轮解密", "密文往返成功")),
        Ok(_) => checks.push(failed("双棘轮解密", "明文不匹配".to_string())),
        Err(err) => checks.push(failed("双棘轮解密", err.to_string())),
    }
    let mut tampered = wire.clone();
    if let Some(last) = tampered.ciphertext.last_mut() {
        *last ^= 0x01;
    }
    checks.push(ProtocolCheck {
        name: "AEAD 篡改检测".to_string(),
        passed: bob_session.decrypt(&tampered).is_err(),
        detail: "修改密文后必须认证失败".to_string(),
    });
    checks
}

fn passed(name: &str, detail: &str) -> ProtocolCheck {
    ProtocolCheck {
        name: name.to_string(),
        passed: true,
        detail: detail.to_string(),
    }
}

fn failed(name: &str, detail: String) -> ProtocolCheck {
    ProtocolCheck {
        name: name.to_string(),
        passed: false,
        detail,
    }
}

fn error_json(error: String) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": error,
    })
}

fn json_to_c_string(value: serde_json::Value) -> *mut c_char {
    let text = serde_json::to_string(&value).unwrap_or_else(|err| {
        serde_json::json!({
            "ok": false,
            "error": err.to_string(),
        })
        .to_string()
    });
    CString::new(text)
        .unwrap_or_else(|_| CString::new("{\"ok\":false,\"error\":\"nul byte\"}").unwrap())
        .into_raw()
}
