# fakeroot

A simple crate which provides the ability to redirect filesystem calls.
This crate builds a library that can be used via `LD_PRELOAD`.

Some examples follow.

**Intercept a file:**
```bash
mkdir /tmp/etc
echo "tee hee" > /tmp/etc/hosts
FAKE_ROOT="/tmp" LD_PRELOAD="path/to/libfakeroot.so" cat /etc/hosts
# tee hee
```

**Intercept a directory list:**
```bash
mkdir /tmp/etc
echo "whatever" > /tmp/etc/🪃
FAKE_ROOT="/tmp" FAKE_DIRS=1 LD_PRELOAD="path/to/libfakeroot.so" ls /etc
# 🪃
```

Options are configured via environment variables:
* `FAKE_ROOT`: absolute path to the fake root
* `FAKE_DIRS`: whether or not to intercept directory listing calls too
* `DEBUG`: if set, will debug log to STDERR

License: GPL-3.0-only
