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
//! * `FAKE_ROOT`: absolute path to the fake root
//! * `FAKE_DIRS`: whether or not to intercept directory listing calls too
//! * `DEBUG`: if set, will debug log to STDERR

use std::cell::OnceCell;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::os::unix::prelude::OsStrExt;
use std::path::PathBuf;
use std::{env, str};

use libc::DIR;
use libc::{c_char, c_int};

/// Required: absolute path to the directory to use as the fake root
pub const ENV_FAKE_ROOT: &str = "FAKE_ROOT";
/// Optional: should this also hook directories?
pub const ENV_FAKE_DIRS: &str = "FAKE_DIRS";
/// Optional: should this hook log debug information to STDERR?
pub const ENV_DEBUG: &str = "DEBUG";

/// Used as a prefix for all debug logs
const HOOK_TAG: &str = "@HOOK@";
/// Runtime cache of the fake root directory
const FAKE_ROOT: OnceCell<Result<PathBuf, Box<dyn Error>>> = OnceCell::new();

macro_rules! log {
    ($($arg:tt)+) => {
        if std::env::var(ENV_DEBUG).is_ok() {
            eprintln!($($arg)*);
        }
    };
}

/// Read the environment variable to know where the fake root directory is.
/// This is used to initialise the `FAKE_ROOT` `OnceCell` constant.
fn get_fake_root() -> Result<PathBuf, Box<dyn Error>> {
    match env::var(ENV_FAKE_ROOT) {
        Ok(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                if path.exists() {
                    Ok(path)
                } else {
                    Err(format!("{} does not exist on disk", ENV_FAKE_ROOT).into())
                }
            } else {
                Err(format!("{} is not absolute", ENV_FAKE_ROOT).into())
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
    let fake_root = match FAKE_ROOT.get_or_init(get_fake_root) {
        Ok(path) => path.to_path_buf(),
        Err(e) => {
            return Err(format!("{}", e).into());
        }
    };

    // make path relative to our fake root
    // trim off leading `/` since `.join` will replace if it finds an absolute path
    let fake_path = fake_root.join(&path_str[1..]);
    if !fake_path.exists() {
        return Err(format!("not in fake root: {}", path_str).into());
    }

    // we found a fake file, return a string representing its path
    log!("{}: {} => {}", HOOK_TAG, path_str, fake_path.display());
    Ok(CString::new(fake_path.as_os_str().as_bytes()).unwrap())
}

// hooks -----------------------------------------------------------------------

// open
redhook::hook! {
    unsafe fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int => my_open {
        let fake = get_fake_path(CStr::from_ptr(path));
        match fake {
            Ok(c_str) => redhook::real!(open)(c_str.as_ptr(), flags, mode),
            Err(e) => {
                log!("{}: {}", HOOK_TAG, e);
                redhook::real!(open)(path, flags, mode)
            },
        }
    }
}

// open64
redhook::hook! {
    unsafe fn open64(path: *const c_char, flags: c_int, mode: c_int) -> c_int => my_open64 {
        let fake = get_fake_path(CStr::from_ptr(path));
        match fake {
            Ok(c_str) => redhook::real!(open64)(c_str.as_ptr(), flags, mode),
            Err(e) => {
                log!("{}: {}", HOOK_TAG, e);
                redhook::real!(open64)(path, flags, mode)
            },
        }
    }
}

redhook::hook! {
    unsafe fn opendir(path: *const c_char) -> *mut DIR => my_opendir {
        if env::var(ENV_FAKE_DIRS).is_err() {
            return redhook::real!(opendir)(path);
        }

        let fake = get_fake_path(CStr::from_ptr(path));
        match fake {
            Ok(c_str) => redhook::real!(opendir)(c_str.as_ptr()),
            Err(e) => {
                log!("{}: {}", HOOK_TAG, e);
                redhook::real!(opendir)(path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        process::{self, Command},
    };

    use super::*;

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

    macro_rules! cmd {
        (
            $fake_root:expr,
            $cmd:expr
            $(, dirs = $dirs:literal)?
            $(, debug = $debug:literal)?
        ) => {{
            let mut cmd = Command::new("sh");
            cmd.arg("-c")
                .arg($cmd)
                .env("LD_PRELOAD", get_so().display().to_string())
                .env(ENV_FAKE_ROOT, $fake_root);
            $(
                if $dirs {
                    cmd.env(ENV_FAKE_DIRS, "1");
                }
            )?
            $(
                if $debug {
                    cmd.env("DEBUG", "1");
                }
            )?
            cmd.output()
                .unwrap()
        }};
    }

    macro_rules! test {
        ($name:ident, $f:expr) => {
            #[test]
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

    test!(simple, |dir: &PathBuf| {
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

    test!(debug, |dir: &PathBuf| {
        let fake_etc = dir.join("etc");
        fs::create_dir_all(&fake_etc).unwrap();
        fs::write(fake_etc.join("hosts"), "🎉").unwrap();

        // this checks ENV_DEBUG behaviour, so ensure it's not set
        assert!(
            env::var(ENV_DEBUG).is_err(),
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
}
