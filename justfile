set positional-arguments

_default:
  just -l

build:
  cargo build

_prepare:
  @mkdir -p root

run *cmd: build _prepare
  LD_PRELOAD="./target/debug/libfakeroot.so" \
    FAKE_ROOT="`pwd`/root" \
    FAKE_DIRS="1" \
    DEBUG="1" \
    "$@"


test *args: build
  cargo test "$@"