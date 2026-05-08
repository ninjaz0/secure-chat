#ifndef SECURE_CHAT_FFI_H
#define SECURE_CHAT_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

char *secure_chat_demo_state_json(void);
char *secure_chat_protocol_self_test_json(void);
char *secure_chat_relay_smoke_json(void);
char *secure_chat_app_snapshot_json(const char *data_dir);
char *secure_chat_app_bootstrap_json(const char *data_dir, const char *display_name, const char *relay_url);
char *secure_chat_app_update_relay_json(const char *data_dir, const char *relay_url);
char *secure_chat_app_invite_json(const char *data_dir);
char *secure_chat_app_preview_invite_json(const char *data_dir, const char *invite_text);
char *secure_chat_app_add_contact_json(const char *data_dir, const char *display_name, const char *invite_uri);
char *secure_chat_app_send_message_json(const char *data_dir, const char *contact_id, const char *body);
char *secure_chat_app_receive_json(const char *data_dir);
void secure_chat_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
