// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas-bench`: times the engine's offline render and prints JSON, with the
//! options and output fields of the reference harness's `ref-bench`.

use std::path::PathBuf;
use std::process::ExitCode;

use sas_bench::{run_bench, wav_bytes, BenchOptions};

const USAGE: &str = "usage: sas-bench [options] <preset.vital|--init> <input.mid|--no-midi>

Times offline renders of a preset driven by a MIDI file, block by block like
sas-render, and prints JSON on stdout. Each repeat builds a fresh synth (not
timed), loads the preset and renders --length seconds; only the block loop
is timed. Build it with --release; see migration/BENCHMARKS.md.

  --preset PATH / --midi PATH / --init / --no-midi   as sas-render
  --sample-rate HZ     default 44100
  --block-size N       host block size, default 64
  --length SECONDS     render length (required)
  --bpm BPM            default 120
  --seed N             engine seed counter before each synth is built, default 1
  --warmup N           untimed renders before the timed ones, default 1
  --repeats N          timed renders, default 3
  -o, --out PATH       also write the last render as a float WAV";

fn parse(args: &[String]) -> Result<(BenchOptions, Option<PathBuf>), String> {
    let mut options = BenchOptions::default();
    let mut out = None;
    let (mut init, mut no_midi) = (false, false);
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{name} needs a value"))
        };
        fn number<T: std::str::FromStr>(name: &str, text: String) -> Result<T, String> {
            text.parse()
                .map_err(|_| format!("{name}: cannot parse {text:?}"))
        }
        match arg {
            "-h" | "--help" => return Err(String::new()),
            "--init" => init = true,
            "--no-midi" => no_midi = true,
            "--preset" => options.preset = Some(value(arg)?.into()),
            "--midi" => options.midi = Some(value(arg)?.into()),
            "-o" | "--out" => out = Some(PathBuf::from(value(arg)?)),
            "--sample-rate" => options.sample_rate = number(arg, value(arg)?)?,
            "--block-size" => options.block_size = number(arg, value(arg)?)?,
            "--length" => options.length = number(arg, value(arg)?)?,
            "--bpm" => options.bpm = number(arg, value(arg)?)?,
            "--seed" => options.seed = number(arg, value(arg)?)?,
            "--warmup" => options.warmup = number(arg, value(arg)?)?,
            "--repeats" => options.repeats = number(arg, value(arg)?)?,
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}")),
            _ => positional.push(PathBuf::from(arg)),
        }
        i += 1;
    }
    let mut positional = positional.into_iter();
    if options.preset.is_none() && !init {
        options.preset = positional.next();
    }
    if options.midi.is_none() && !no_midi {
        options.midi = positional.next();
    }
    if let Some(extra) = positional.next() {
        return Err(format!("unexpected argument {}", extra.display()));
    }
    if options.preset.is_none() && !init {
        return Err("a preset is required (or pass --init)".to_string());
    }
    if options.midi.is_none() && !no_midi {
        return Err("a MIDI file is required (or pass --no-midi)".to_string());
    }
    Ok((options, out))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, out) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("sas-bench: {error}");
            }
            eprintln!("{USAGE}");
            return if error.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    let bench = match run_bench(&options) {
        Ok(bench) => bench,
        Err(error) => {
            eprintln!("sas-bench: {error}");
            return ExitCode::from(2);
        }
    };
    if let Some(out) = out {
        if let Err(error) = std::fs::write(&out, wav_bytes(bench.output(), options.sample_rate)) {
            eprintln!("sas-bench: cannot write {}: {error}", out.display());
            return ExitCode::from(2);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&bench.to_json()).expect("JSON")
    );
    if bench.non_finite() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
