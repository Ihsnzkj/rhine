#!/usr/bin/env python3
"""Check the post-authentication part of a PAM service for the current user
(pam_acct_mgmt + pam_setcred REFRESH) — no password, no authentication, so it
can never count towards pam_faillock.  Guards against a service file without
an `account` section, which falls through to /etc/pam.d/other (= deny).

  tools/pam-check.py [service]      (default: rhine-lock)  exit 0 = PAM_SUCCESS
"""
import ctypes, ctypes.util, getpass, sys

pam = ctypes.CDLL(ctypes.util.find_library("pam"))
pam.pam_strerror.restype = ctypes.c_char_p


class Message(ctypes.Structure):
    _fields_ = [("msg_style", ctypes.c_int), ("msg", ctypes.c_char_p)]


class Response(ctypes.Structure):
    _fields_ = [("resp", ctypes.c_char_p), ("resp_retcode", ctypes.c_int)]


CONV_FUNC = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_int, ctypes.POINTER(ctypes.POINTER(Message)),
                             ctypes.POINTER(ctypes.POINTER(Response)), ctypes.c_void_p)


class Conv(ctypes.Structure):
    _fields_ = [("conv", CONV_FUNC), ("appdata_ptr", ctypes.c_void_p)]


@CONV_FUNC
def conversation(n, msgs, resp, data):
    return 19  # PAM_CONV_ERR: the account stack must not prompt


service = sys.argv[1] if len(sys.argv) > 1 else "rhine-lock"
handle = ctypes.c_void_p()
conv = Conv(conversation, None)
rc = pam.pam_start(service.encode(), getpass.getuser().encode(), ctypes.byref(conv), ctypes.byref(handle))
if rc != 0:
    sys.exit(f"pam_start: {pam.pam_strerror(handle, rc).decode()}")
rc = pam.pam_acct_mgmt(handle, 0)
print(f"{service}: pam_acct_mgmt → {rc} ({pam.pam_strerror(handle, rc).decode()})")
# PAM_REFRESH_CRED: the step rhine-lock runs after a successful authentication
rc2 = pam.pam_setcred(handle, 0x0010)
print(f"{service}: pam_setcred(REFRESH) → {rc2} ({pam.pam_strerror(handle, rc2).decode()})")
pam.pam_end(handle, rc)
sys.exit(0 if rc == 0 and rc2 == 0 else 1)
