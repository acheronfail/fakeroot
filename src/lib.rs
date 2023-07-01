//! A simple crate which provides the ability to redirect filesystem calls.
//! This crate builds a library that can be used via `LD_PRELOAD`.
//!
//! Some examples follow.
//!
//! **Intercept a file:**
//! ```bash
//! mkdir /tmp/etc
//! echo "tee hee" > /tmp/etc/hosts
//! FAKE_ROOT="/tmp" LD_PRELOAD="path/to/libfakeroot.so" cat /etc/hosts
//! # tee hee
//! ```
//!
//! **Intercept a directory list:**
//! ```bash
//! mkdir /tmp/etc
//! echo "whatever" > /tmp/etc/🪃
//! FAKE_ROOT="/tmp" FAKE_DIRS=1 LD_PRELOAD="path/to/libfakeroot.so" ls /etc
//! # 🪃
//! ```
//!
//! Options are configured via environment variables:
//! * `FAKEROOT`: absolute path to the fake root
//! * `FAKEROOT_DIRS`: whether or not to intercept directory listing calls too
//! * `FAKEROOT_ALL`: whether or not to fake non-existent files and directories
//! * `FAKEROOT_DEBUG`: if set, will debug log to STDERR

use std::cell::OnceCell;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::os::unix::prelude::OsStrExt;
use std::path::PathBuf;
use std::{env, str};

use libc::{c_char, c_int};
use libc::{DIR, FILE};

/// Required: absolute path to the directory to use as the fake root
pub const ENV_FAKEROOT: &str = "FAKEROOT";
/// Optional: should this also hook directories?
pub const ENV_FAKEROOT_DIRS: &str = "FAKEROOT_DIRS";
/// Optional: should non existent files be faked?
pub const ENV_FAKEROOT_ALL: &str = "FAKEROOT_ALL";
/// Optional: should this hook log debug information to STDERR?
pub const ENV_FAKEROOT_DEBUG: &str = "FAKEROOT_DEBUG";

/// Used as a prefix for all debug logs
const HOOK_TAG: &str = "@HOOK@";
/// Runtime cache of the fake root directory
const FAKEROOT_ROOT: OnceCell<Result<PathBuf, Box<dyn Error>>> = OnceCell::new();
/// Runtime cache of debug state
const FAKEROOT_DEBUG: OnceCell<bool> = OnceCell::new();

macro_rules! log {
    ($($arg:tt)+) => {
        if *FAKEROOT_DEBUG.get_or_init(|| is_enabled(ENV_FAKEROOT_DEBUG)) {
            eprintln!($($arg)*);
        }
    };
}

/// Read the environment variable to know where the fake root directory is.
/// This is used to initialise the `FAKEROOT_ROOT` `OnceCell` constant.
fn get_fake_root() -> Result<PathBuf, Box<dyn Error>> {
    match env::var(ENV_FAKEROOT) {
        Ok(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                if path.exists() {
                    Ok(path)
                } else {
                    Err(format!("{} does not exist on disk", ENV_FAKEROOT).into())
                }
            } else {
                Err(format!("{} is not absolute", ENV_FAKEROOT).into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Return a `CString` if a file exists in the fake root for the given string.
fn get_fake_path(c_str: &CStr) -> Result<CString, Box<dyn Error>> {
    // parse c string
    let path_str = match str::from_utf8(c_str.to_bytes()) {
        Ok(actual_path) => actual_path,
        Err(e) => {
            return Err(format!("failed to read string: {}", e).into());
        }
    };

    // get fake root
    let fake_root = match FAKEROOT_ROOT.get_or_init(get_fake_root) {
        Ok(path) => path.to_path_buf(),
        Err(e) => {
            return Err(format!("{}", e).into());
        }
    };

    // make path relative to our fake root
    // trim off leading `/` since `.join` will replace if it finds an absolute path
    let fake_path = fake_root.join(&path_str[1..]);

    // bail out if the file doesn't exist and `ENV_FAKEROOT_ALL` isn't enabled
    if !is_enabled(ENV_FAKEROOT_ALL) && !fake_path.exists() {
        return Err(format!("not in fake root: {}", path_str).into());
    }

    // we found a fake file, return a string representing its path
    log!("{}: {} => {}", HOOK_TAG, path_str, fake_path.display());
    Ok(CString::new(fake_path.as_os_str().as_bytes()).unwrap())
}

fn is_enabled(env_key: &str) -> bool {
    match env::var(env_key) {
        Ok(val) => val != "false" && val != "0",
        Err(_) => false,
    }
}

// macros ----------------------------------------------------------------------

macro_rules! do_hook {
    ($name:ident => $($before_arg:ident, )* [$path:ident] $(, $after_arg:ident)* $(,)?) => {
        do_hook!($name if true => $($before_arg, )* [$path] $(, $after_arg)*)
    };

    ($name:ident if $cond:expr => $($before_arg:ident, )* [$path:ident] $(, $after_arg:ident)* $(,)?) => {{
        let real = redhook::real!($name);
        match get_fake_path(CStr::from_ptr($path)) {
            Ok(c_str) if $cond => real($($before_arg, )* c_str.as_ptr() $(, $after_arg)*),
            Ok(_) => real($($before_arg, )* $path $(, $after_arg)*),
            Err(e) => {
                log!("{}: {}", HOOK_TAG, e);
                real($($before_arg, )* $path $(, $after_arg)*)
            },
        }
    }};
}

// hooks -----------------------------------------------------------------------

// open
redhook::hook! {
    unsafe fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int => my_open {
        do_hook!(open => [path], flags, mode)
    }
}

// open64
redhook::hook! {
    unsafe fn open64(path: *const c_char, flags: c_int, mode: c_int) -> c_int => my_open64 {
        do_hook!(open64 => [path], flags, mode)
    }
}

// fopen
redhook::hook! {
    unsafe fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE => my_fopen {
        do_hook!(fopen => [path], mode)
    }
}

// opendir
redhook::hook! {
    unsafe fn opendir(path: *const c_char) -> *mut DIR => my_opendir {
        do_hook!(opendir if is_enabled(ENV_FAKEROOT_DIRS) => [path])
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        process::{self, Command},
    };

    use super::*;

    #[test]
    fn test_is_enabled() {
        let test_var = "test_var";

        env::remove_var(test_var);
        assert_eq!(is_enabled(test_var), false);

        env::set_var(test_var, "false");
        assert_eq!(is_enabled(test_var), false);

        env::set_var(test_var, "0");
        assert_eq!(is_enabled(test_var), false);

        env::set_var(test_var, "true");
        assert_eq!(is_enabled(test_var), true);

        env::set_var(test_var, "1");
        assert_eq!(is_enabled(test_var), true);

        env::set_var(test_var, "anything");
        assert_eq!(is_enabled(test_var), true);
    }

    // NOTE: this requires that `cargo build` be run before the tests are run
    // - is there a way to use one that's built when the tests are built?
    fn get_so() -> PathBuf {
        env::current_exe() // target/debug/deps/<file>
            .unwrap()
            .parent() // target/debug/deps
            .unwrap()
            .parent() // target/debug
            .unwrap()
            .join("libfakeroot.so")
    }

    macro_rules! cat {
        ($p:expr) => {
            fs::read_to_string($p).unwrap()
        };
    }

    macro_rules! cmd {
        (
            $fake_root:expr,
            $cmd:expr
            $(, all = $all:literal)?
            $(, dirs = $dirs:literal)?
            $(, debug = $debug:literal)?
            $(,)?
        ) => {{
            let mut cmd = Command::new("sh");
            cmd.arg("-c")
                .arg($cmd)
                .env("LD_PRELOAD", get_so().display().to_string())
                .env(ENV_FAKEROOT, $fake_root);

            $(
                if $all {
                    cmd.env(ENV_FAKEROOT_ALL, "1");
                }
            )?

            $(
                if $dirs {
                    cmd.env(ENV_FAKEROOT_DIRS, "1");
                }
            )?

            $(
                if $debug {
                    cmd.env(ENV_FAKEROOT_DEBUG, "1");
                }
            )?

            let output = cmd.output()
                .unwrap();

            let success = output.status.success();
            if !success {
                assert!(
                    false,
                    "\"{}\" -> {}\n{}",
                    $cmd,
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }

            output
        }};
    }

    // TODO: include doc comments so can add #[should_panic] on top
    macro_rules! test {
        ($(#[$($attr:tt)+] )? $name:ident, $f:expr) => {
            #[test]
            $(#[$($attr)+])?
            fn $name() {
                let tmp_dir = env::temp_dir().join(format!(
                    "fakehook-{}-{}",
                    stringify!($name),
                    process::id()
                ));
                std::fs::create_dir_all(&tmp_dir).unwrap();
                $f(&tmp_dir);
                std::fs::remove_dir_all(&tmp_dir).unwrap();
            }
        };
    }

    test!(simple, |dir: &Path| {
        let fake_etc = dir.join("etc");
        fs::create_dir_all(&fake_etc).unwrap();
        fs::write(fake_etc.join("hosts"), "🎉").unwrap();

        // check hook worked
        let output = cmd!(&dir, "cat /etc/hosts");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "🎉");

        // check other files aren't hooked
        let output = cmd!(&dir, "cat /etc/passwd", debug = true);
        assert_eq!(output.stdout, fs::read("/etc/passwd").unwrap());
    });

    test!(debug, |dir: &Path| {
        let fake_etc = dir.join("etc");
        fs::create_dir_all(&fake_etc).unwrap();
        fs::write(fake_etc.join("hosts"), "🎉").unwrap();

        // this checks ENV_DEBUG behaviour, so ensure it's not set
        assert!(
            env::var(ENV_FAKEROOT_DEBUG).is_err(),
            "DEBUG must not be defined during tests"
        );

        // should be no logs
        let output = cmd!(&dir, "cat /etc/hosts");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");

        // should be logs
        let output = cmd!(&dir, "cat /etc/passwd", debug = true);
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("@HOOK@: not in fake root: /etc/passwd"));
    });

    test!(dir, |dir: &PathBuf| {
        let fake_etc = dir.join("etc");
        fs::create_dir_all(&fake_etc).unwrap();
        fs::write(fake_etc.join("FAKED"), "💥").unwrap();

        // check dir not hooked
        let output = cmd!(&dir, "ls /etc");
        assert_ne!(String::from_utf8_lossy(&output.stdout).trim(), "FAKED");

        // check dir hooked
        let output = cmd!(&dir, "ls /etc", dirs = true);
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "FAKED");
    });

    // tests fopen by using `tee`
    // https://github.com/coreutils/coreutils/blob/master/src/tee.c#L263
    test!(fopen, |dir: &Path| {
        let fake_opt = dir.join("opt");
        fs::create_dir_all(&fake_opt).unwrap();
        fs::write(fake_opt.join("foo"), "not 1").unwrap();
        fs::write(fake_opt.join("bar"), "not 1").unwrap();

        cmd!(
            &dir,
            "echo 1 | tee /opt/{foo,bar}",
            dirs = true,
            debug = true
        );
        assert_eq!(cat!(fake_opt.join("foo")).trim(), "1");
        assert_eq!(cat!(fake_opt.join("bar")).trim(), "1");
    });

    test!(all, |fake_dir: &Path| {
        cmd!(&fake_dir, "echo 1 > /asdf", all = true);
        assert_eq!(cat!(fake_dir.join("asdf")).trim(), "1");
    });

    test!(
        #[should_panic(expected = "/asdf: Permission denied")]
        all_unset,
        |fake_dir: &Path| {
            cmd!(&fake_dir, "echo 1 > /asdf");
        }
    );
}
