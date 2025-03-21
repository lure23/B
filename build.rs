/*
* build.rs
*
* Gets run by:
*   - IDE on host; WRONG FEATURES!!
*   - 'cargo build' (CLI); correct features
*/
use anyhow::*;

use std::{
    env,
    fs,
    process::Command
};

// Snippets need to be read in here (cannot do in "statement position")
//
include!("build_snippets/pins.in");

const CONFIG_H_NEXT: &str = "tmp/config.h.next";

/*
* Note: 'build.rs' is supposedly run only once, for any 'examples', 'lib' etc. build.
*
* References:
*   - Environment variables set
*       -> https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-build-scripts
*/
fn main() -> Result<()> {
    // Detect when IDE is running us:
    //  - Rust Rover:
    //      __CFBundleIdentifier=com.jetbrains.rustrover-EAP
    //
    #[allow(non_snake_case)]
    let IDE_RUN = std::env::var("__CFBundleIdentifier").is_ok();

    // If IDE runs, terminate early.
    if IDE_RUN { return Ok(()) };

    // DEBUG: Show what we know about the compilation.
    //
    // <<
    //   CARGO_CFG_TARGET_FEATURE=c,m
    //   CARGO_FEATURE_{..feature..}=1
    //   LD_LIBRARY_PATH=/home/ubuntu/VL53L5CX_rs.cifs/vl53l5cx_uld/target/release/deps:/home/ubuntu/VL53L5CX_rs.cifs/vl53l5cx_uld/target/release:/home/ubuntu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib:/home/ubuntu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib
    //   RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu
    //   TARGET=riscv32imc-unknown-none-elf
    // <<
    #[cfg(not(all()))]
    {
        std::env::vars().for_each(|(a, b)| { eprintln!("{a}={b}"); });
        panic!();
    }

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

    //---
    // Config sanity checks
    {
        // In the nature of Rust features being combinable, several 'targets_per_zone{2..4}' _are_ allowed, the
        // grandest of them making the call. = we don't need to check for sanity.
        //
        //"targets_per_zone_2",
        //"targets_per_zone_3",
        //"targets_per_zone_4",

        // "range_sigma_mm" relates to "distance_mm"
        #[cfg(all(feature = "range_sigma_mm", not(feature = "distance_mm")))]
        println!("cargo:warning=Feature 'range_sigma_mm' does not make sense without feature 'distance_mm' (which is not enabled)");
    }

    // Config sanity checks (if 'examples/*')
    //
    if std::env::var("EXAMPLE").is_ok() {   // "EXAMPLE=m3"
        #[cfg(not(any(feature = "run_with_espflash", feature = "run_with_probe_rs")))]
        panic!("Must enable feature: run_with_{{espflash|probe_rs}}");

        #[cfg(all(feature = "run_with_espflash", feature = "run_with_probe_rs"))]
        panic!("Must enable ONLY one of features: run_with_{{espflash|probe_rs}}");
    }

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

    //---
    // Create a C config header, reflecting the Rust-side features required.
    //
    // MUST BE BEFORE running the Makefile.
    //
    // Note: Never run this on IDE builds - the features a person selects in the IDE UI don't necessarily match 
    //       what the real builds will be about.
    {
        use itertools::Itertools;
        let mut defs: Vec<String> = vec!();

        macro_rules! add {
            ($x:expr) => { defs.push($x.into()); }
        }
        // ^-- Practically the same as:
        // let add = |s: dyn Into<String>| { defs.push(s.into()) };    // does not compile

        // Output-enabling features (in Rust, we have them enabling; in C they are disable flags). Same thing.
        //
        // First group: metadata of the sensor (DIMxDIM, regardless of targets)
        //
        #[cfg(not(feature = "ambient_per_spad"))]
        add!("VL53L5CX_DISABLE_AMBIENT_PER_SPAD");
        #[cfg(not(feature = "nb_spads_enabled"))]
        add!("VL53L5CX_DISABLE_NB_SPADS_ENABLED");
        #[cfg(not(feature = "nb_targets_detected"))]
        add!("VL53L5CX_DISABLE_NB_TARGET_DETECTED");
        //
        // Second group: data and metadata (DIMxDIMxTARGETS)
        //
        #[cfg(not(feature = "target_status"))]
        add!("VL53L5CX_DISABLE_TARGET_STATUS");
        #[cfg(not(feature = "distance_mm"))]
        add!("VL53L5CX_DISABLE_DISTANCE_MM");
        #[cfg(not(feature = "range_sigma_mm"))]
        add!("VL53L5CX_DISABLE_RANGE_SIGMA_MM");
        #[cfg(not(feature = "reflectance_percent"))]
        add!("VL53L5CX_DISABLE_REFLECTANCE_PERCENT");
        #[cfg(not(feature = "signal_per_spad"))]
        add!("VL53L5CX_DISABLE_SIGNAL_PER_SPAD");

        // 'motion_indicator' support is not implemented; always disable in C
        add!("VL53L5CX_DISABLE_MOTION_INDICATOR");

        // Vendor docs:
        //      "the number of target[s] per zone sent through I2C. [...] a lower number [...] means a lower RAM
        //      [consumption]."
        //
        // NOTE: In the nature of Rust features being *combinable* (the merger matters; features should not be
        //      exclusive), we use the *largest* given feature. If there are none, 1.
        //
        const TARGETS: usize =
                 if cfg!(feature = "targets_per_zone_4") { 4 }
            else if cfg!(feature = "targets_per_zone_3") { 3 }
            else if cfg!(feature = "targets_per_zone_2") { 2 }
            else { 1 };     // always one target

        defs.push(format!("VL53L5CX_NB_TARGET_PER_ZONE {TARGETS}U"));

        // Write the file. This way the last 'cargo build' state remains available, even if
        // 'make' were run manually (compared to passing individual defines to 'make');
        // also, it keeps the 'Makefile' simple.
        //
        let contents = defs.iter()
            .map(|s| format!("#define {s}"))
            .join("\n");

        fs::write(CONFIG_H_NEXT, contents)
            .expect( &format!("Unable to write {}", CONFIG_H_NEXT) );
    }

    // make stuff
    //
    let st = Command::new("make")
        //.arg("-B")
        .arg("tmp/libvendor_uld.a")    // ULD C library
        .arg("tmp/uld_raw.rs")      // generate the ULD Rust bindings
        .output()
        .expect("to be able to launch `make`")   // shown if 'make' not found on PATH
        .status;

    if !st.success() {
        // Remove "tmp/config.h[.next]" so they will get recreated next time. This should avoid
        // the build to get in an awkward position where the developer needs to remove them, themselves.
        //
        fs::remove_file("tmp/config.h")?;
        fs::remove_file(CONFIG_H_NEXT)?;

        panic!("[ERROR!]: Running 'make' failed. \
            SUGGESTION: run 'make manual' on the command line to see more error information. \
        ");
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

        if std::env::var("TEST").is_ok() {  // 'cargo test' run
            println!("cargo::rustc-link-arg-tests=-Tembedded-test.x");
        }
    }

    println!("cargo:rustc-link-search=tmp");
    println!("cargo:rustc-link-lib=static=vendor_uld");

    Ok(())
}
