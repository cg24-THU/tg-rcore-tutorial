fn main() {
    use std::{env, fs, io::Seek, io::SeekFrom, path::PathBuf};

    if env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() != "riscv64" {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let linker = out_dir.join("linker.ld");
    fs::write(&linker, LINKER_SCRIPT).unwrap();
    println!("cargo:rustc-link-arg=-T{}", linker.display());

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let disk_dir = manifest_dir.join("target").join("riscv64gc-unknown-none-elf").join("debug");
    fs::create_dir_all(&disk_dir).unwrap();
    let disk_path = disk_dir.join("disk.img");
    if !disk_path.exists() {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&disk_path)
            .unwrap();
        file.seek(SeekFrom::Start((128 * 512 - 1) as u64)).unwrap();
        std::io::Write::write_all(&mut file, &[0]).unwrap();
    }
}

const LINKER_SCRIPT: &[u8] = b"
OUTPUT_ARCH(riscv)
ENTRY(_m_start)

M_BASE_ADDRESS = 0x80000000;
S_BASE_ADDRESS = 0x80200000;

SECTIONS {
    . = M_BASE_ADDRESS;
    .text.m_entry : { *(.text.m_entry) }
    .text.m_trap  : { *(.text.m_trap)  }
    .bss.m_stack  : { *(.bss.m_stack)  }
    .bss.m_data   : { *(.bss.m_data)   }

    . = S_BASE_ADDRESS;
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    .bss : {
        *(.bss.uninit)
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }
}";
