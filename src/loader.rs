use anyhow::Result;
use goblin::elf::{Elf, section_header, sym::Sym};
use scroll::{Pwrite, ctx::SizeWith};
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;

// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
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

    let file = fs::File::open("/proc/kallsyms")?;
    let reader = std::io::BufReader::new(file);

    let allsyms = reader
        .lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| {
            let mut splits = line.split_whitespace();
            let addr = u64::from_str_radix(splits.next()?, 16).ok()?;
            let symbol = splits.nth(1)?;
            let symbol_trimmed = symbol
                .find("$")
                .or_else(|| symbol.find(".llvm."))
                .map_or(symbol, |pos| &symbol[0..pos])
                .to_owned();
            Some((symbol_trimmed, addr))
        })
        .collect::<HashMap<_, _>>();

    Ok(allsyms)
}

pub fn load_module(path: &str, params: Option<&str>) -> Result<(), (ErrorCode, String)> {
    let mut buffer = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return Err((ErrorCode::ReadFileFailed, e.to_string())),
    };
    let elf = match Elf::parse(&buffer) {
        Ok(e) => e,
        Err(e) => return Err((ErrorCode::ReadElfFailed, e.to_string())),
    };

    let kernel_symbols = match parse_kallsyms() {
        Ok(ks) => ks,
        Err(e) => return Err((ErrorCode::ParseKallsymsFailed, e.to_string())),
    };

    let mut modifications = Vec::new();
    for (index, mut sym) in elf.syms.iter().enumerate() {
        if index == 0 {
            continue;
        }

        if sym.st_shndx != section_header::SHN_UNDEF as usize {
            continue;
        }

        let Some(name) = elf.strtab.get_at(sym.st_name) else {
            continue;
        };

        let offset = elf.syms.offset() + index * Sym::size_with(elf.syms.ctx());
        let Some(real_addr) = kernel_symbols.get(name) else {
            eprintln!("WARN: Cannot find symbol: {}", &name);
            continue;
        };
        sym.st_shndx = section_header::SHN_ABS as usize;
        sym.st_value = *real_addr;
        modifications.push((sym, offset));
    }

    let ctx = *elf.syms.ctx();
    for ele in modifications {
        if buffer.pwrite_with(ele.0, ele.1, ctx).is_err() {
            return Err((ErrorCode::AppendElfFailed, "pwrite_with failed".to_string()));
        }
    }

    let params_c = std::ffi::CString::new(params.unwrap_or("")).unwrap_or_default();
    match rustix::system::init_module(&buffer, &params_c) {
        Ok(()) => Ok(()),
        Err(e) => Err((ErrorCode::InitModuleFailed, e.to_string())),
    }
}
