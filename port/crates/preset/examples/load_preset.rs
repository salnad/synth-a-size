// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads a preset file and prints its saved state (or why it failed to load).
//!
//! `cargo run -p sas-preset --example load_preset -- path/to/preset.vital`

use sas_preset::{InitContent, PresetState};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: load_preset <preset.vital>");
    let mut state = PresetState::new(&InitContent {
        wavetables: Vec::new(),
        sample_seed: 0,
    });
    match state.load_file(std::path::Path::new(&path)) {
        Ok(()) => println!("{}", state.to_json_string()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(3);
        }
    }
}
