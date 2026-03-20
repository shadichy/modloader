use anyhow::Result;
use scroll::{Pwrite, ctx::SizeWith};
use std::collections::HashMap;
use std::fs;

// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    Success = 0,
    InvalidProcess = 1,
    ReadFileFailed = 2,
    ReadElfFailed = 3,
    AppendElfFailed = 4,
    ParseKallsymsFailed = 5,
    InitModuleFailed = 6,
}

struct Kptr {
    value: String,
}

impl Kptr {
    pub fn new() -> Result<Self> {
        let value = fs::read_to_string("/proc/sys/kernel/kptr_restrict")?;
        fs::write("/proc/sys/kernel/kptr_restrict", "1")?;
        Ok(Kptr { value })
    }
}

impl Drop for Kptr {
    fn drop(&mut self) {
        let _ = fs::write("/proc/sys/kernel/kptr_restrict", self.value.as_bytes());
    }
}

fn parse_kallsyms() -> Result<HashMap<String, u64>> {
    let _dontdrop = Kptr::new()?;

    let allsyms = fs::read_to_string("/proc/kallsyms")?
        .lines()
        .map(|line| line.split_whitespace())
        .filter_map(|mut splits| {
            splits
                .next()
                .and_then(|addr| u64::from_str_radix(addr, 16).ok())
                .and_then(|addr| splits.nth(1).map(|symbol| (symbol, addr)))
        })
        .map(|(symbol, addr)| {
            (
                symbol
                    .find("$")
                    .or_else(|| symbol.find(".llvm."))
                    .map_or(symbol, |pos| &symbol[0..pos])
                    .to_owned(),
                addr,
            )
        })
        .collect::<HashMap<_, _>>();

    Ok(allsyms)
}

pub fn load_module(path: &str, params: Option<&str>) -> ErrorCode {
    let mut buffer = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return ErrorCode::ReadFileFailed,
    };
    let elf = match goblin::elf::Elf::parse(&buffer) {
        Ok(e) => e,
        Err(_) => return ErrorCode::ReadElfFailed,
    };

    let kernel_symbols = match parse_kallsyms() {
        Ok(ks) => ks,
        Err(_) => return ErrorCode::ParseKallsymsFailed,
    };

    let mut modifications = Vec::new();
    for (index, mut sym) in elf.syms.iter().enumerate() {
        if index == 0 {
            continue;
        }

        if sym.st_shndx != goblin::elf::section_header::SHN_UNDEF as usize {
            continue;
        }

        let Some(name) = elf.strtab.get_at(sym.st_name) else {
            continue;
        };

        let offset = elf.syms.offset() + index * goblin::elf::sym::Sym::size_with(elf.syms.ctx());
        let Some(real_addr) = kernel_symbols.get(name) else {
            eprintln!("WARN: Cannot find symbol: {}", &name);
            continue;
        };
        sym.st_shndx = goblin::elf::section_header::SHN_ABS as usize;
        sym.st_value = *real_addr;
        modifications.push((sym, offset));
    }

    let ctx = *elf.syms.ctx();
    for ele in modifications {
        if buffer.pwrite_with(ele.0, ele.1, ctx).is_err() {
            return ErrorCode::AppendElfFailed;
        }
    }

    let params_c = std::ffi::CString::new(params.unwrap_or("")).unwrap_or_default();
    match rustix::system::init_module(&buffer, &params_c) {
        Ok(()) => ErrorCode::Success,
        Err(_) => ErrorCode::InitModuleFailed,
    }
}
