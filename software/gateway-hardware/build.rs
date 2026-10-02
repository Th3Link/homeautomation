use std::process::Command;

fn main() {
    vergen().unwrap();
    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    create_esp32_image();
}

fn create_esp32_image() {
    // Cargo build scripts always run with their *package* directory as
    // cwd, never the workspace root — a plain relative
    // "target/xtensa-esp32-none-elf/..." (which is where this crate's
    // output actually lands, since it's a workspace member sharing the
    // root target/) resolves to the wrong,
    // software/gateway-hardware/target/... that doesn't exist. OUT_DIR is
    // always correct and absolute (Cargo sets it); the profile directory
    // is 3 ancestors up from it:
    // <target_dir>/<triple>/<profile>/build/<pkg>-<hash>/out.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let profile_dir = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR has at least 3 ancestors")
        .to_path_buf();
    let target_binary = profile_dir.join("gateway-hardware");
    let ota_binary = profile_dir.join("gateway-hardware.bin");

    // This can't succeed on the very first build of a fresh checkout —
    // build scripts run *before* their own crate links, so
    // `target_binary` won't exist yet. It picks up the previous build's
    // ELF on every subsequent build instead; good enough for CI, which
    // always builds twice in a row for debug and release.
    println!("cargo:rerun-if-changed={}", target_binary.display());
    if let Err(e) = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            "esp32",
            &target_binary.to_string_lossy(),
            &ota_binary.to_string_lossy(),
        ])
        .status()
    {
        println!("cargo:warning=Failed to create ESP32 image: {e}");
        // No panic! - build should still succeed without the image.
    }
}

fn vergen() -> Result<(), Box<dyn std::error::Error>> {
    let git2 = vergen_git2::Git2::all_git();
    vergen_git2::Emitter::default()
        .add_instructions(&git2)?
        .emit()?;

    Ok(())
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
