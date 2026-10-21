// SPDX-License-Identifier: GPL-3.0-or-later

//! Prints the saved state after loading a preset file (or the init preset),
//! or why the load failed.
//!
//! `cargo run -p sas-preset --example load_preset -- [--seed N] (path/to/preset.vital | --init)`
//!
//! `--seed` is the white-noise sample seed of the init state (default 0).

use sas_preset::{InitContent, PresetState};

fn main() {
    let mut sample_seed = 0;
    let mut target = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--seed" {
            sample_seed = args
                .next()
                .and_then(|seed| seed.parse().ok())
                .expect("--seed needs an integer");
        } else {
            target = Some(arg);
        }
    }
    let target = target.expect("usage: load_preset [--seed N] (<preset.vital> | --init)");

    let mut state = PresetState::new(&InitContent { sample_seed });
    if target != "--init" {
        if let Err(error) = state.load_file(std::path::Path::new(&target)) {
            eprintln!("{error}");
            std::process::exit(3);
        }
    }
    println!("{}", state.to_json_string());
}
