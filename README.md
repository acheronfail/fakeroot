[![crate](https://img.shields.io/crates/v/fakeroot)](https://crates.io/crates/fakeroot)
[![documentation](https://docs.rs/fakeroot/badge.svg)](https://docs.rs/fakeroot)

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
* `FAKEROOT`: absolute path to the fake root
* `FAKEROOT_DIRS`: whether or not to intercept directory listing calls too
* `FAKEROOT_ALL`: whether or not to fake non-existent files and directories
* `FAKEROOT_DEBUG`: if set, will debug log to STDERR

License: GPL-3.0-only