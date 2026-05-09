#ifndef SECURE_CHAT_FFI_H
#define SECURE_CHAT_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

char *secure_chat_demo_state_json(void);
char *secure_chat_protocol_self_test_json(void);
char *secure_chat_relay_smoke_json(void);
char *secure_chat_p2p_smoke_json(void);
char *secure_chat_app_snapshot_json(const char *data_dir);
char *secure_chat_app_bootstrap_json(const char *data_dir, const char *display_name, const char *relay_url);
char *secure_chat_app_update_relay_json(const char *data_dir, const char *relay_url);
char *secure_chat_app_invite_json(const char *data_dir);
char *secure_chat_app_temporary_invite_json(const char *data_dir);
char *secure_chat_app_preview_invite_json(const char *data_dir, const char *invite_text);
char *secure_chat_app_start_temporary_connection_json(const char *data_dir, const char *invite_uri);
char *secure_chat_app_add_contact_json(const char *data_dir, const char *display_name, const char *invite_uri);
char *secure_chat_app_update_contact_display_name_json(const char *data_dir, const char *contact_id, const char *display_name);
char *secure_chat_app_delete_contact_json(const char *data_dir, const char *contact_id);
char *secure_chat_app_send_temporary_message_json(const char *data_dir, const char *connection_id, const char *body);
char *secure_chat_app_end_temporary_connection_json(const char *data_dir, const char *connection_id);
char *secure_chat_app_send_message_json(const char *data_dir, const char *contact_id, const char *body);
char *secure_chat_app_send_attachment_json(const char *data_dir, const char *thread_kind, const char *thread_id, const char *file_path, const char *kind);
char *secure_chat_app_send_burn_message_json(const char *data_dir, const char *thread_kind, const char *thread_id, const char *body);
char *secure_chat_app_open_burn_message_json(const char *data_dir, const char *thread_kind, const char *thread_id, const char *message_id);
char *secure_chat_app_create_group_json(const char *data_dir, const char *display_name);
char *secure_chat_app_add_group_member_json(const char *data_dir, const char *group_id, const char *contact_id);
char *secure_chat_app_send_group_message_json(const char *data_dir, const char *group_id, const char *body);
char *secure_chat_app_register_push_token_json(const char *data_dir, const char *token, const char *platform);
char *secure_chat_app_import_sticker_json(const char *data_dir, const char *file_path, const char *display_name);
char *secure_chat_app_delete_sticker_json(const char *data_dir, const char *sticker_id);
char *secure_chat_app_receive_json(const char *data_dir);
char *secure_chat_app_p2p_probe_json(const char *data_dir);
void secure_chat_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
