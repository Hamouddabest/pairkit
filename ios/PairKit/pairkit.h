#ifndef PAIRKIT_H
#define PAIRKIT_H

typedef void (*pk_log_cb)(const char *line, void *ctx);
typedef char *(*pk_ask_cb)(const char *label, int secret, void *ctx);
typedef void (*pk_pin_cb)(const char *pin, void *ctx);
typedef void (*pk_done_cb)(int ok, const char *message, void *ctx);

void pk_free(char *p);
char *pk_pair_start(pk_log_cb log, pk_pin_cb pin, pk_done_cb done, void *ctx);
void pk_pair_stop(void);
char *pk_install(const char *ipa_path, const char *opts_json, pk_log_cb log, pk_ask_cb ask, void *ctx);

char *pk_manage_app_ids(const char *opts_json, pk_log_cb log, pk_ask_cb ask, void *ctx);

#endif
