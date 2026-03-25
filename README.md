# Linux Kernel Module Loader

[![Build and Release](https://github.com/shadichy/modloader/actions/workflows/build-release.yml/badge.svg)](https://github.com/shadichy/modloader/actions/workflows/build-release.yml)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/shadichy/modloader)](https://github.com/shadichy/modloader/releases/latest)

`modloader` is a specialized utility designed to patch and load Linux Kernel Modules (LKMs) entirely in user-space. While originally developed for the KernelSU ecosystem, it serves as a robust tool for any environment where kernel module symbols need to be resolved against the running kernel's symbol table (`kallsyms`) before loading.

# How it Works

Standard kernel module loading (`insmod` and `modprobe`) relies on the kernel's internal linker to resolve symbols. On many locked or restricted Android/Linux systems, this process can fail if the module wasn't compiled against the exact kernel headers or if symbol signing is enforced.

`modloader` bypasses these restrictions by performing the linking process in user-space:

1.  **Permission Escalation**: Temporarily sets `/proc/sys/kernel/kptr_restrict` to `1` (if run as root) to ensure kernel addresses are visible.
2.  **Symbol Resolution**: Parses `/proc/kallsyms` using a memory-efficient streaming reader to build a map of the running kernel's symbols.
3.  **ELF Patching**: Reads the target `.ko` file, identifies `SHN_UNDEF` (undefined) symbols, and patches their values and section indices (`SHN_ABS`) directly in the memory buffer using the addresses found in `kallsyms`.
4.  **Loading**: Executes the `init_module` system call with the patched buffer and any provided module parameters.
5.  **Validation (Optional)**: If built with the `kernelsu` feature, it performs a post-load check using the `reboot` and `prctl` syscalls to verify and report the KernelSU version.

## System Requirements

For `modloader` to function correctly, the target Linux kernel must be compiled with the following configurations enabled:

*   `CONFIG_MODULES=y`: Support for loadable kernel modules.
*   `CONFIG_KALLSYMS=y`: Support for the kernel symbol table.
*   `CONFIG_PROC_FS=y`: Access to the `/proc` filesystem (needed for `kallsyms` and `sysctl`).
*   `CONFIG_SYSCTL=y`: Support for kernel parameter tuning via `sysctl`.

## For KernelSU

`modloader` is specifically optimized for **KernelSU LKM Mode**. When KernelSU is compiled as a module (`CONFIG_KSU=m`), it often needs to be loaded into kernels that do not export the necessary symbols or enforce strict signature checks (SELinux policies).

Key benefits for KernelSU users:
- **Symbol Patching**: KernelSU requires access to internal kernel symbols (like `sys_read` or `vfs_write`) that are often not exported for module use. `modloader` patches these addresses at runtime.
- **GKI Compatibility**: Allows running KernelSU on Generic Kernel Images (GKI) or stock kernels without needing a complete kernel rebuild.
- **Stability**: Ensures the module is correctly linked against the running kernel's exact memory layout, preventing "Exec format error" or "Required key not available" failures.

When the `kernelsu` feature is enabled, `modloader` verifies the installation using two methods:
- **Modern Method (v2+)**: Uses a modified `reboot` system call with magic numbers (`0xDEADBEEF`, `0xCAFEBABE`) to retrieve a file descriptor for the KernelSU driver. It then uses `ioctl` to fetch version and feature information.
- **Legacy Method**: Uses a modified `prctl` system call with the `0xDEADBEEF` command to directly retrieve the version number from the kernel.

# Installation

## Debian / Ubuntu

Prebuilt `.deb` packages are available in [Ananda-Aropa/modloader](https://github.com/Ananda-Aropa/modloader/releases) for easy installation.

**Quick Install:**
```bash
# Download using curl
curl -Lo modloader.deb https://github.com/Ananda-Aropa/modloader/releases/download/2.0.0-3/modloader_2.0.0-1_amd64.deb

# Or using wget
wget -O modloader.deb https://github.com/Ananda-Aropa/modloader/releases/download/2.0.0-3/modloader_2.0.0-1_amd64.deb

# Install with dpkg
sudo dpkg -i modloader.deb
```

## Arch Linux

`modloader` is available in the Arch User Repository (AUR). You can use an AUR helper like `paru` or `yay`, or build manually.

**Using an AUR helper:**
```bash
paru -S modloader
# or
yay -S modloader
```

**Building manually with makepkg:**
```bash
git clone --depth 1 https://aur.archlinux.org/modloader.git
cd modloader
makepkg -si
```

## Generic Linux

Download the appropriate binary for your architecture and C library from the [GitHub Releases](https://github.com/shadichy/modloader/releases).

## Supported Artifacts

| Architecture | libc | Target Triple |
| :--- | :--- | :--- |
| **x86_64** | musl (static) | `x86_64-unknown-linux-musl` |
| **aarch64** | musl (static) | `aarch64-unknown-linux-musl` |
| **x86_64** | glibc | `x86_64-unknown-linux-gnu` |
| **aarch64** | glibc | `aarch64-unknown-linux-gnu` |
| **aarch64** | bionic (Android) | `aarch64-linux-android` |
| **x86_64** | bionic (Android) | `x86_64-linux-android` |

```bash
# Example for x86_64 static binary
wget https://github.com/shadichy/modloader/releases/latest/download/modloader-x86_64-musl
chmod +x modloader-x86_64-musl
sudo ./modloader-x86_64-musl /path/to/module.ko [params]
```

# Usage

```bash
sudo modloader <path_to_module.ko> [module_parameters...]
```

**Example:**
```bash
sudo modloader my_module.ko param1=value1 param2=value2
```

Use `-` as a path to read the module from `stdin`:
```bash
cat my_module.ko | sudo modloader -
```

# Build Guide

## Prerequisites
- [Rust](https://rustup.rs/) (Stable)
- [cross-rs](https://github.com/cross-rs/cross) (*optional:* for cross-compilation)

## Feature Flags

| Feature | Description | Default |
| :--- | :--- | :--- |
| `kernelsu` | Enables KernelSU version verification after loading. | Disabled |

## Local Build (Native)

**Debug Mode:**
```bash
cargo build
# or with KernelSU support
cargo build --features kernelsu
```
The binary will be located at `target/debug/modloader`.

> **Note:** For production builds, use the `--release` flag (binary will be in `target/release/modloader`).

## Cross-Compilation

**Debug Mode:**
```bash
cross build --target aarch64-unknown-linux-musl --features kernelsu
cross build --target aarch64-linux-android --features kernelsu
```
Binaries will be located at `target/<target>/debug/modloader`.

> **Note:** For production builds, use the `--release` flag (binaries will be in `target/<target>/release/modloader`).


# Technical Notes
- **Static Linking**: The `musl` targets are fully static, making them ideal for recovery environments or minimal Android systems.
- **Entry Point**: This project uses `#![no_main]` and a C-style `main` function to avoid overhead and issues related to standard Rust runtime initialization on certain systems.

## Error Codes

`modloader` uses specific exit codes to indicate different failure states. Exit codes for module loading errors are bit-shifted (`code << 2`).

| Exit Code | Meaning | Description |
| :--- | :--- | :--- |
| **0** | Success | Module loaded and (if enabled) KernelSU verified. |
| **1** | Permission Denied | The program must be run as root (UID 0). |
| **2** | Invalid Arguments | Incorrect CLI usage or invalid module path. |
| **3** | KernelSU Error | Module loaded, but KernelSU version check failed. |
| **4** | Invalid Process | Internal error during process handling. |
| **8** | Read File Failed | Could not read the specified `.ko` file. |
| **12** | Read ELF Failed | The file is not a valid ELF or is corrupted. |
| **16** | Append ELF Failed | Failed to patch the ELF symbol table in memory. |
| **20** | Parse Kallsyms Failed | Could not read or parse `/proc/kallsyms`. |
| **24** | Init Module Failed | The `init_module` syscall returned an error (e.g., `ENOEXEC`). |
