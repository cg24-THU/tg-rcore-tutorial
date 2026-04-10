fn main() {
    use std::{env, fs, path::PathBuf};

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BASE_ADDRESS");

    if let Some(base) = env::var("BASE_ADDRESS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        let script = format!(
            "\
OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {{
    . = {base};
    .text : {{
        *(.text.entry)
        *(.text .text.*)
    }}
    .rodata : {{
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }}
    .data : {{
        *(.data .data.*)
        *(.sdata .sdata.*)
    }}
    .bss : {{
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }}
}}"
        );
        let linker = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("linker.ld");
        fs::write(&linker, script).unwrap();
        println!("cargo:rustc-link-arg=-T{}", linker.display());
    }
}
