use secure_chat_core::{
    accept_session_as_responder_consuming_prekey, safety_number, start_session_as_initiator,
    ApnsPlatform, CipherSuite, DeviceKeyMaterial, Invite, ObfuscationProfile, PlainMessage,
    TransportFrame,
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
pub extern "C" fn secure_chat_app_temporary_invite_json(data_dir: *const c_char) -> *mut c_char {
    json_to_c_string(match c_arg(data_dir, "data_dir") {
        Ok(data_dir) => match secure_chat_desktop::DesktopRuntime::temporary_invite(data_dir) {
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
pub extern "C" fn secure_chat_app_start_temporary_connection_json(
    data_dir: *const c_char,
    invite_uri: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (c_arg(data_dir, "data_dir"), c_arg(invite_uri, "invite_uri")) {
            (Ok(data_dir), Ok(invite_uri)) => {
                match secure_chat_desktop::DesktopRuntime::start_temporary_connection(
                    data_dir,
                    &invite_uri,
                ) {
                    Ok(response) => to_value(response),
                    Err(err) => error_json(err.to_string()),
                }
            }
            (data_dir, invite_uri) => error_json(
                data_dir
                    .err()
                    .or_else(|| invite_uri.err())
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
pub extern "C" fn secure_chat_app_send_temporary_message_json(
    data_dir: *const c_char,
    connection_id: *const c_char,
    body: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(connection_id, "connection_id"),
            c_arg(body, "body"),
        ) {
            (Ok(data_dir), Ok(connection_id), Ok(body)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::send_temporary_message(
                        data_dir,
                        &connection_id,
                        &body,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, connection_id, body) => error_json(
                data_dir
                    .err()
                    .or_else(|| connection_id.err())
                    .or_else(|| body.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_end_temporary_connection_json(
    data_dir: *const c_char,
    connection_id: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(connection_id, "connection_id"),
        ) {
            (Ok(data_dir), Ok(connection_id)) => {
                match secure_chat_desktop::DesktopRuntime::end_temporary_connection(
                    data_dir,
                    &connection_id,
                ) {
                    Ok(snapshot) => to_value(snapshot),
                    Err(err) => error_json(err.to_string()),
                }
            }
            (data_dir, connection_id) => error_json(
                data_dir
                    .err()
                    .or_else(|| connection_id.err())
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
pub extern "C" fn secure_chat_app_create_group_json(
    data_dir: *const c_char,
    display_name: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(display_name, "display_name"),
        ) {
            (Ok(data_dir), Ok(display_name)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::create_group(
                        data_dir,
                        &display_name,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, display_name) => error_json(
                data_dir
                    .err()
                    .or_else(|| display_name.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_add_group_member_json(
    data_dir: *const c_char,
    group_id: *const c_char,
    contact_id: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(group_id, "group_id"),
            c_arg(contact_id, "contact_id"),
        ) {
            (Ok(data_dir), Ok(group_id), Ok(contact_id)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::add_group_member(
                        data_dir,
                        &group_id,
                        &contact_id,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, group_id, contact_id) => error_json(
                data_dir
                    .err()
                    .or_else(|| group_id.err())
                    .or_else(|| contact_id.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_send_group_message_json(
    data_dir: *const c_char,
    group_id: *const c_char,
    body: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(group_id, "group_id"),
            c_arg(body, "body"),
        ) {
            (Ok(data_dir), Ok(group_id), Ok(body)) => desktop_async(|runtime| {
                runtime
                    .block_on(secure_chat_desktop::DesktopRuntime::send_group_message(
                        data_dir, &group_id, &body,
                    ))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            }),
            (data_dir, group_id, body) => error_json(
                data_dir
                    .err()
                    .or_else(|| group_id.err())
                    .or_else(|| body.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        },
    )
}

#[no_mangle]
pub extern "C" fn secure_chat_app_register_push_token_json(
    data_dir: *const c_char,
    token: *const c_char,
    platform: *const c_char,
) -> *mut c_char {
    json_to_c_string(
        match (
            c_arg(data_dir, "data_dir"),
            c_arg(token, "token"),
            c_arg(platform, "platform"),
        ) {
            (Ok(data_dir), Ok(token), Ok(platform)) => {
                let platform = match platform.as_str() {
                    "macos" => ApnsPlatform::Macos,
                    _ => ApnsPlatform::Ios,
                };
                desktop_async(|runtime| {
                    runtime
                        .block_on(secure_chat_desktop::DesktopRuntime::register_push_token(
                            data_dir, &token, platform,
                        ))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            }
            (data_dir, token, platform) => error_json(
                data_dir
                    .err()
                    .or_else(|| token.err())
                    .or_else(|| platform.err())
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

#[cfg(target_os = "android")]
mod android_jni {
    use super::{desktop_async, error_json, json_to_c_string, run_protocol_checks, to_value};
    use jni::objects::{JClass, JString};
    use jni::sys::jstring;
    use jni::JNIEnv;
    use secure_chat_desktop::DesktopRuntime;

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_protocolSelfTestJson(
        mut env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let checks = run_protocol_checks();
        json_out(
            &mut env,
            serde_json::json!({
                "ok": checks.iter().all(|check| check.passed),
                "checks": checks,
            }),
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_relaySmokeJson(
        mut env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let value = match tokio::runtime::Runtime::new() {
            Ok(runtime) => match runtime.block_on(secure_chat_client::run_relay_smoke()) {
                Ok(report) => to_value(report),
                Err(err) => error_json(err.to_string()),
            },
            Err(err) => error_json(err.to_string()),
        };
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_appSnapshotJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let value = with_1(
            &mut env,
            data_dir,
            "data_dir",
            |data_dir| match DesktopRuntime::open(data_dir).and_then(|runtime| runtime.snapshot()) {
                Ok(snapshot) => to_value(snapshot),
                Err(err) => error_json(err.to_string()),
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_bootstrapJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        display_name: JString,
        relay_url: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            display_name,
            "display_name",
            relay_url,
            "relay_url",
            |data_dir, display_name, relay_url| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::bootstrap(
                            data_dir,
                            &display_name,
                            &relay_url,
                        ))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_updateRelayJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        relay_url: JString,
    ) -> jstring {
        let value = with_2(
            &mut env,
            data_dir,
            "data_dir",
            relay_url,
            "relay_url",
            |data_dir, relay_url| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::update_relay(data_dir, &relay_url))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_inviteJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let value = with_1(&mut env, data_dir, "data_dir", |data_dir| {
            DesktopRuntime::invite(data_dir)
                .map(to_value)
                .unwrap_or_else(|err| error_json(err.to_string()))
        });
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_temporaryInviteJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let value = with_1(&mut env, data_dir, "data_dir", |data_dir| {
            DesktopRuntime::temporary_invite(data_dir)
                .map(to_value)
                .unwrap_or_else(|err| error_json(err.to_string()))
        });
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_previewInviteJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        invite_text: JString,
    ) -> jstring {
        let value = with_2(
            &mut env,
            data_dir,
            "data_dir",
            invite_text,
            "invite_text",
            |data_dir, invite_text| {
                DesktopRuntime::preview_invite(data_dir, &invite_text)
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_addContactJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        display_name: JString,
        invite_uri: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            display_name,
            "display_name",
            invite_uri,
            "invite_uri",
            |data_dir, display_name, invite_uri| {
                DesktopRuntime::add_contact(data_dir, &display_name, &invite_uri)
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_startTemporaryConnectionJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        invite_uri: JString,
    ) -> jstring {
        let value = with_2(
            &mut env,
            data_dir,
            "data_dir",
            invite_uri,
            "invite_uri",
            |data_dir, invite_uri| {
                DesktopRuntime::start_temporary_connection(data_dir, &invite_uri)
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_sendMessageJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        contact_id: JString,
        body: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            contact_id,
            "contact_id",
            body,
            "body",
            |data_dir, contact_id, body| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::send_message(data_dir, &contact_id, &body))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_createGroupJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        display_name: JString,
    ) -> jstring {
        let value = with_2(
            &mut env,
            data_dir,
            "data_dir",
            display_name,
            "display_name",
            |data_dir, display_name| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::create_group(data_dir, &display_name))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_addGroupMemberJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        group_id: JString,
        contact_id: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            group_id,
            "group_id",
            contact_id,
            "contact_id",
            |data_dir, group_id, contact_id| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::add_group_member(
                            data_dir,
                            &group_id,
                            &contact_id,
                        ))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_sendGroupMessageJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        group_id: JString,
        body: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            group_id,
            "group_id",
            body,
            "body",
            |data_dir, group_id, body| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::send_group_message(
                            data_dir, &group_id, &body,
                        ))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_sendTemporaryMessageJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        connection_id: JString,
        body: JString,
    ) -> jstring {
        let value = with_3(
            &mut env,
            data_dir,
            "data_dir",
            connection_id,
            "connection_id",
            body,
            "body",
            |data_dir, connection_id, body| {
                desktop_async(|runtime| {
                    runtime
                        .block_on(DesktopRuntime::send_temporary_message(
                            data_dir,
                            &connection_id,
                            &body,
                        ))
                        .map(to_value)
                        .unwrap_or_else(|err| error_json(err.to_string()))
                })
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_endTemporaryConnectionJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        connection_id: JString,
    ) -> jstring {
        let value = with_2(
            &mut env,
            data_dir,
            "data_dir",
            connection_id,
            "connection_id",
            |data_dir, connection_id| {
                DesktopRuntime::end_temporary_connection(data_dir, &connection_id)
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            },
        );
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_receiveJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let value = with_1(&mut env, data_dir, "data_dir", |data_dir| {
            desktop_async(|runtime| {
                runtime
                    .block_on(DesktopRuntime::receive(data_dir))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            })
        });
        json_out(&mut env, value)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_securechat_android_core_SecureChatNative_p2pProbeJson(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let value = with_1(&mut env, data_dir, "data_dir", |data_dir| {
            desktop_async(|runtime| {
                runtime
                    .block_on(DesktopRuntime::p2p_probe(data_dir))
                    .map(to_value)
                    .unwrap_or_else(|err| error_json(err.to_string()))
            })
        });
        json_out(&mut env, value)
    }

    fn with_1(
        env: &mut JNIEnv,
        value: JString,
        name: &str,
        work: impl FnOnce(String) -> serde_json::Value,
    ) -> serde_json::Value {
        match jstring_arg(env, value, name) {
            Ok(value) => work(value),
            Err(err) => error_json(err),
        }
    }

    fn with_2(
        env: &mut JNIEnv,
        first: JString,
        first_name: &str,
        second: JString,
        second_name: &str,
        work: impl FnOnce(String, String) -> serde_json::Value,
    ) -> serde_json::Value {
        match (
            jstring_arg(env, first, first_name),
            jstring_arg(env, second, second_name),
        ) {
            (Ok(first), Ok(second)) => work(first, second),
            (first, second) => error_json(
                first
                    .err()
                    .or_else(|| second.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        }
    }

    fn with_3(
        env: &mut JNIEnv,
        first: JString,
        first_name: &str,
        second: JString,
        second_name: &str,
        third: JString,
        third_name: &str,
        work: impl FnOnce(String, String, String) -> serde_json::Value,
    ) -> serde_json::Value {
        match (
            jstring_arg(env, first, first_name),
            jstring_arg(env, second, second_name),
            jstring_arg(env, third, third_name),
        ) {
            (Ok(first), Ok(second), Ok(third)) => work(first, second, third),
            (first, second, third) => error_json(
                first
                    .err()
                    .or_else(|| second.err())
                    .or_else(|| third.err())
                    .unwrap_or_else(|| "invalid arguments".to_string()),
            ),
        }
    }

    fn jstring_arg(env: &mut JNIEnv, value: JString, name: &str) -> Result<String, String> {
        env.get_string(&value)
            .map(|value| value.into())
            .map_err(|err| format!("{name} is not valid UTF-8: {err}"))
    }

    fn json_out(env: &mut JNIEnv, value: serde_json::Value) -> jstring {
        let pointer = json_to_c_string(value);
        if pointer.is_null() {
            return std::ptr::null_mut();
        }
        let text = unsafe { std::ffi::CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            drop(std::ffi::CString::from_raw(pointer));
        }
        env.new_string(text)
            .map(|value| value.into_raw())
            .unwrap_or(std::ptr::null_mut())
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
    let mut bob = DeviceKeyMaterial::generate(8);
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
    let mut bob_session = accept_session_as_responder_consuming_prekey(&mut bob, &initial)
        .map_err(|err| err.to_string())?;
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
    let mut bob = DeviceKeyMaterial::generate(4);
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
    let mut bob_session = match accept_session_as_responder_consuming_prekey(&mut bob, &initial) {
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
