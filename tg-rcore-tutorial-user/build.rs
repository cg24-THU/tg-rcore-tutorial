fn main() {
    use std::{env, fs, path::PathBuf};

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LOG");
    println!("cargo:rerun-if-env-changed=BASE_ADDRESS");
    println!("cargo:rerun-if-env-changed=CHAPTER");

    if let Ok(chapter) = env::var("CHAPTER") {
        println!("cargo:rustc-env=CHAPTER={chapter}");
    }

    if let Some(base) = env::var("BASE_ADDRESS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        let text = format!(
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
        let ld = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("linker.ld");
        fs::write(&ld, text).unwrap();
        println!("cargo:rustc-link-arg=-T{}", ld.display());
    }

    if env::var("CARGO_CFG_TARGET_ARCH").ok().as_deref() == Some("riscv64") {
        build_doom_support();
    }
}

fn build_doom_support() {
    use std::{env, path::PathBuf};

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let doom_dir = manifest_dir.join("vendor").join("doomgeneric");
    let shim = manifest_dir.join("src").join("doom_shim.c");
    println!("cargo:rerun-if-changed={}", doom_dir.display());
    println!("cargo:rerun-if-changed={}", shim.display());

    if !doom_dir.exists() {
        panic!(
            "vendor/doomgeneric not found at {}; copy DoomGeneric sources before building",
            doom_dir.display()
        );
    }

    let gcc = "riscv64-unknown-elf-gcc";
    let ar = "riscv64-unknown-elf-ar";
    let multidir = command_output(gcc, ["-print-multi-directory"]);
    let libgcc = PathBuf::from(command_output(gcc, ["-print-libgcc-file-name"]));
    let libgcc_dir = libgcc.parent().unwrap().to_path_buf();
    let picolibc_root = PathBuf::from("/usr/lib/picolibc/riscv64-unknown-elf");
    let picolibc_include = picolibc_root.join("include");
    let picolibc_lib = {
        let candidate = picolibc_root.join("lib").join(multidir.trim());
        if candidate.exists() {
            candidate
        } else {
            picolibc_root.join("lib")
        }
    };

    let mut build = cc::Build::new();
    build.compiler(gcc);
    build.archiver(ar);
    build.flag("-march=rv64gc");
    build.flag("-mabi=lp64d");
    build.flag("-O2");
    build.flag("-ffunction-sections");
    build.flag("-fdata-sections");
    build.flag("-fno-stack-protector");
    build.flag("-DNORMALUNIX");
    build.flag("-DLINUX");
    build.flag("-DSNDSERV");
    build.flag("-D_DEFAULT_SOURCE");
    build.include(&doom_dir);
    build.include(&picolibc_include);
    for source in doom_sources() {
        build.file(doom_dir.join(source));
    }
    build.file(shim);
    build.compile("doomgeneric");

    println!("cargo:rustc-link-search=native={}", libgcc_dir.display());
    println!("cargo:rustc-link-search=native={}", picolibc_lib.display());
    println!("cargo:rustc-link-lib=static=doomgeneric");
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=m");
    println!("cargo:rustc-link-lib=static=gcc");
    println!("cargo:rustc-link-lib=static=dummyhost");
}

fn command_output<const N: usize>(program: &str, args: [&str; N]) -> String {
    use std::process::Command;

    let output = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("failed to run {program}: {err}"));
    if !output.status.success() {
        panic!("{program} exited with status {}", output.status);
    }
    String::from_utf8(output.stdout)
        .unwrap_or_else(|err| panic!("failed to decode {program} output: {err}"))
        .trim()
        .to_string()
}

fn doom_sources() -> &'static [&'static str] {
    &[
        "dummy.c",
        "am_map.c",
        "doomdef.c",
        "doomstat.c",
        "dstrings.c",
        "d_event.c",
        "d_items.c",
        "d_iwad.c",
        "d_loop.c",
        "d_main.c",
        "d_mode.c",
        "d_net.c",
        "f_finale.c",
        "f_wipe.c",
        "g_game.c",
        "hu_lib.c",
        "hu_stuff.c",
        "info.c",
        "i_cdmus.c",
        "i_endoom.c",
        "i_joystick.c",
        "i_scale.c",
        "i_sound.c",
        "i_system.c",
        "i_timer.c",
        "memio.c",
        "m_argv.c",
        "m_bbox.c",
        "m_cheat.c",
        "m_config.c",
        "m_controls.c",
        "m_fixed.c",
        "m_menu.c",
        "m_misc.c",
        "m_random.c",
        "p_ceilng.c",
        "p_doors.c",
        "p_enemy.c",
        "p_floor.c",
        "p_inter.c",
        "p_lights.c",
        "p_map.c",
        "p_maputl.c",
        "p_mobj.c",
        "p_plats.c",
        "p_pspr.c",
        "p_saveg.c",
        "p_setup.c",
        "p_sight.c",
        "p_spec.c",
        "p_switch.c",
        "p_telept.c",
        "p_tick.c",
        "p_user.c",
        "r_bsp.c",
        "r_data.c",
        "r_draw.c",
        "r_main.c",
        "r_plane.c",
        "r_segs.c",
        "r_sky.c",
        "r_things.c",
        "sha1.c",
        "sounds.c",
        "statdump.c",
        "st_lib.c",
        "st_stuff.c",
        "s_sound.c",
        "tables.c",
        "v_video.c",
        "wi_stuff.c",
        "w_checksum.c",
        "w_file.c",
        "w_main.c",
        "w_wad.c",
        "z_zone.c",
        "w_file_stdc.c",
        "i_input.c",
        "i_video.c",
        "doomgeneric.c",
    ]
}
