#![no_main]

mod loader;
#[cfg(feature = "kernelsu")]
mod checker;

use std::ffi::{CStr, c_char};
use rustix::process::getuid;
use loader::ErrorCode;

#[cfg(feature = "kernelsu")]
use crate::checker::has_kernelsu;

// Error code to message mapping
const ERROR_MESSAGES: &[(&str, ErrorCode)] = &[
    ("Invalid process", ErrorCode::InvalidProcess),
    ("Could not read file", ErrorCode::ReadFileFailed),
    ("Could not read ELF header of file", ErrorCode::ReadElfFailed),
    ("Could not parse kallsyms", ErrorCode::ParseKallsymsFailed),
    ("Could not append modifications to ELF", ErrorCode::AppendElfFailed),
    ("Module init failed", ErrorCode::InitModuleFailed),
];

/// # Safety
/// This is the entry point of the program
/// We cannot use the main because rust will abort if we don't have std{in/out/err}
/// https://github.com/rust-lang/rust/blob/3071aefdb2821439e2e6f592f41a4d28e40c1e79/library/std/src/sys/unix/mod.rs#L80
/// So we use the C main function and call rust code from there
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: i32, argv: *const *const c_char, _envp: *const *const c_char) -> i32 {
    // Program name
    let _p_abs = unsafe { CStr::from_ptr(*argv) }.to_str().unwrap();
    let _p = _p_abs.split('/').last().unwrap();
    const _E: &str = "ERROR";

    // Check if running as root (uid 0)
    let uid = getuid();
    if uid.as_raw() != 0 {
        eprintln!("{}: {}: {}", _p, _E, "Insufficient permission. Please run as root.");
        return 1;
    }

    // Check if we have at least 2 arguments (program name + path)
    if argc < 2 {
        eprintln!("{}: {}: {}", _p, _E, "Invalid arguments.");
        eprintln!("{}: {}: '{} {}'", _p, "Usage", _p_abs, "path/to/module.ko [params]");
        return 2;
    }

    // Get the path argument
    let path_cstr = unsafe { CStr::from_ptr(*(argv.add(1))) };
    let path_str = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("{}: {}: {} '{}'.", _p, _E, "Invalid argument", path_cstr.to_string_lossy());
            return 2;
        }
    };

    // Preprocess: if argument is '-', replace with /dev/stdin
    let path = if path_str == "-" {
        "/dev/stdin"
    } else {
        path_str
    };

    // Concat all arguments from argv[2] into params
    let mut params = String::new();
    for i in 2..argc {
        let arg = unsafe { CStr::from_ptr(*(argv.add(i as usize))) }.to_str().unwrap_or("");
        if !params.is_empty() {
            params.push(' ');
        }
        params.push_str(arg);
    }

    // Call loader and get result
    let result = loader::load_module(path, Some(&params));

    // If successful, return 0
    if result.is_ok() {
        #[cfg(feature = "kernelsu")]
        {
            let version = has_kernelsu();
            if version == 0 {
                eprintln!("{}: {}: {}", _p, _E, "Invalid KernelSU version.");
                return 3;
            }

            eprintln!("{}: {}: {}.", _p, "KernelSU Version", version);
        }
        return 0;
    }

    let (error_code, detail) = result.unwrap_err();

    // Map error code to message and print
    for (message, code) in ERROR_MESSAGES {
        if *code == error_code {
            eprintln!("{}: {}: {}: {}: {}", _p, _E, path_str, message, detail);
            break;
        }
    }

    (error_code as i32) << 2
}
