// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas-render`: renders a preset and a MIDI file to a WAV offline, with a
//! subset of the reference harness's `ref-render` options and its defaults
//! (no `--warmup-samples`, `--format`, `--meta` or `--dump-state`).

use std::path::PathBuf;
use std::process::ExitCode;

use sas_plugin::render::{render_to_wav, RenderOptions};

const USAGE: &str =
    "usage: sas-render [options] <preset.vital|--init> <input.mid|--no-midi> -o <out.wav>

  --preset PATH        preset file (or first positional argument)
  --midi PATH          MIDI file (or second positional argument)
  --init               render the engine's init state instead of a preset
  --no-midi            render with no MIDI input
  -o, --out PATH       output WAV path (required; 32-bit float stereo)
  --sample-rate HZ     default 44100
  --block-size N       host block size, default 64 (engine sub-blocks of <=128)
  --length SECONDS     total output length; default: last MIDI event + --tail
  --tail SECONDS       extra time after the last MIDI event, default 2.0
  --bpm BPM            tempo for tempo-synced modulators, default 120
  --seed N             set the engine's random seed counter before the synth
                       is built (as ref-render's --seed)

A subset of ref-render's options: no --warmup-samples, --format (always f32),
--meta or --dump-state.";

fn parse(args: &[String]) -> Result<(RenderOptions, PathBuf), String> {
    let mut options = RenderOptions::default();
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
            "--length" => options.length = Some(number(arg, value(arg)?)?),
            "--tail" => options.tail = number(arg, value(arg)?)?,
            "--bpm" => options.bpm = number(arg, value(arg)?)?,
            "--seed" => options.seed = Some(number(arg, value(arg)?)?),
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
    let out = out.ok_or("an output path is required (-o out.wav)")?;
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
                eprintln!("sas-render: {error}");
            }
            eprintln!("{USAGE}");
            return if error.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    match render_to_wav(&options, &out) {
        Ok(render) => {
            println!(
                "{}: {} frames @ {} Hz, peak {:.6}",
                out.display(),
                render.frames(),
                render.sample_rate,
                render.peak()
            );
            if render.dropped_events > 0 {
                eprintln!(
                    "sas-render: {} MIDI events fall after the end",
                    render.dropped_events
                );
            }
            if render.non_finite() > 0 {
                eprintln!("sas-render: {} non-finite samples", render.non_finite());
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sas-render: {error}");
            ExitCode::FAILURE
        }
    }
}
