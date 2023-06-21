//! A simple crate which provides the ability to redirect filesystem calls.
//! This crate builds a library that can be used via `LD_PRELOAD`.
//!
//! An example:
//! ```bash
//! mkdir /tmp/etc
//! echo "tee hee" > /tmp/etc/hosts
//! FAKE_ROOT="/tmp" LD_PRELOAD="path/to/libfakeroot.so" cat /etc/hosts
//! # tee hee
//! ```

use std::cell::OnceCell;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::os::unix::prelude::OsStrExt;
use std::path::PathBuf;
use std::{env, str};

use libc::{c_char, c_int};

/// Required: absolute path to the directory to use as the fake root
pub const ENV_FAKE_ROOT: &str = "FAKE_ROOT";
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
        return Err("no file found in fake root".into());
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

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        process::{self, Command},
    };

    fn get_so() -> PathBuf {
        env::current_exe() // target/debug/deps/<file>
            .unwrap()
            .parent() // target/debug/deps
            .unwrap()
            .parent() // target/debug
            .unwrap()
            .join("libfakeroot.so")
    }

    #[test]
    fn it_works() {
        // create fake root with fake /etc/hosts
        let tmp_dir = env::temp_dir().join(format!("fakehook-{}", process::id()));
        let fake_etc = tmp_dir.join("etc");
        fs::create_dir_all(&fake_etc).unwrap();
        fs::write(fake_etc.join("hosts"), "🎉").unwrap();

        let output = Command::new("cat")
            .arg("/etc/hosts")
            .env("LD_PRELOAD", get_so().display().to_string())
            .env("FAKE_ROOT", &tmp_dir)
            .output()
            .unwrap();

        assert_eq!(String::from_utf8_lossy(&output.stdout), "🎉");
    }
}