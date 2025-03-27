/*
* build.rs
*
* Gets run by:
*   - IDE on host; WRONG FEATURES!!
*   - 'cargo build' (CLI); correct features
*/
use anyhow::*;

// Snippets need to be read in here (cannot do in "statement position")
//
include!("build_snippets/pins.in");

fn main() -> Result<()> {
    use std::{
        env,
        fs,
        process::Command
    };

    // Detect when IDE is running us:
    //  - Rust Rover:
    //      __CFBundleIdentifier=com.jetbrains.rustrover-EAP
    //
    #[allow(non_snake_case)]
    let IDE_RUN = std::env::var("__CFBundleIdentifier").is_ok();

    // If IDE runs, terminate early.
    if IDE_RUN { return Ok(()) };

    // Pick the current MCU. To be used as board id for 'pins.toml'.
    //
    // $ grep -oE -m 1 '"esp32(c3|c6)"' Cargo.toml | cut -d '"' -f2
    //  esp32c3
    //
    let board_id: String = {
        let output = Command::new("sh")
            .arg("-c")
            .arg("grep -oE -m 1 '\"esp32(c3|c6)\"' Cargo.toml | cut -d '\"' -f2")
            .output()
            .expect("'sh' to run");

        // 'output.stdout' is a 'Vec<u8>' (since, well, could be binary)
        //
        let us: &[u8] = output.stdout.as_slice().trim_ascii();
        let x = String::from_utf8_lossy(us);

        //: println!("cargo:warning=BOARD ID: '{}'", &x);     // BOARD ID: 'esp32c3'
        x.into()
    };

    // Expose 'OUT_DIR' to an external (Makefile) build system
    {
        const TMP: &str = ".OUT_DIR";

        let out_dir = env::var("OUT_DIR")
            .expect("OUT_DIR to have a value");

        fs::write(TMP, out_dir)
            .expect(format!("Unable to write {TMP}").as_str());
    }

    //---
    // Turn 'pins.toml' -> 'src/pins_gen.in’ (named within the TOML itself)
    {
        let toml = include_str!("./pins.toml");
        process_pins(toml, &board_id)?;
    }

    // Link arguments
    //
    {
        for s in [
            "-Tlinkall.x",
            "-Tdefmt.x"     // required by 'defmt'
        ] {
            println!("cargo::rustc-link-arg={}", s);
        }
    }

    println!("cargo:rustc-link-search=tmp");
    println!("cargo:rustc-link-lib=static=vendor_uld");

    Ok(())
}
