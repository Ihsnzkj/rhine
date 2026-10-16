/* Deterministic --check-pam fixture. No system PAM policy or credentials. */
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int pam_start(const char *service, const char *user, const void *conversation, void **handle) {
    (void)conversation;
    *handle = NULL;
    /* The executable must use its UID, not our deliberately false USER. */
    if (!user || strcmp(user, "rhine-nobody") == 0) return 3;
    if (strcmp(service, "fixture-start-denied") == 0) return 3;
    *handle = strdup(service);
    return *handle ? 0 : 5;
}
int pam_authenticate(void *handle, int flags) {
    (void)handle; (void)flags;
    _exit(99); /* A preflight regression must never authenticate a real user. */
}
int pam_acct_mgmt(void *handle, int flags) {
    (void)flags;
    if (strcmp(handle, "fixture-hang") == 0) sleep(60);
    return strcmp(handle, "fixture-account-denied") == 0 ? 7 : 0;
}
int pam_setcred(void *handle, int flags) {
    if (flags != 0x10) return 3;
    return strcmp(handle, "fixture-credentials-denied") == 0 ? 7 : 0;
}
int pam_end(void *handle, int status) {
    (void)status;
    free(handle);
    return 0;
}
const char *pam_strerror(void *handle, int status) {
    (void)handle;
    return status == 0 ? "Fixture accepted" : "Fixture refused";
}
