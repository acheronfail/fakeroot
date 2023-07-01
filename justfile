set positional-arguments

badge-crates := "[![crate](https://img.shields.io/crates/v/fakeroot)](https://crates.io/crates/fakeroot)"
badge-docs := "[![documentation](https://docs.rs/fakeroot/badge.svg)](https://docs.rs/fakeroot)"

_default:
  just -l

# prepare for local development
setup:
  cargo install cargo-readme

# builds the crate
build:
  cargo build

# run a command with libfakeroot injected
run *cmd: build
  @mkdir -p root
  LD_PRELOAD="./target/debug/libfakeroot.so" \
    FAKEROOT="`pwd`/root" \
    FAKEROOT_DIRS="1" \
    FAKEROOT_DEBUG="1" \
    "$@"

# test the crate
test *args: build
  cargo test "$@"

# publish the crate
publish: test
  printf "%s\n%s\n\n%s" "{{ badge-crates }}" "{{ badge-docs }}" "$(cargo readme)" > README.md
  cargo publish