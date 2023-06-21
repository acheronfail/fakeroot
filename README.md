# fakeroot

A simple crate which provides the ability to redirect filesystem calls.
This crate builds a library that can be used via `LD_PRELOAD`.

An example:
```bash
mkdir /tmp/etc
echo "tee hee" > /tmp/etc/hosts
FAKE_ROOT="/tmp" LD_PRELOAD="path/to/libfakeroot.so" cat /etc/hosts
# tee hee
```

License: GPL-3.0-only
