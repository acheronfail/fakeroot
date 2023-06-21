set positional-arguments

_default:
  just -l

test *args:
  cargo build
  cargo test "$@"