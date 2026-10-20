use std::path::PathBuf;
use std::process::ExitCode;

use audio_diff::{compare, load, Options, Report, Tolerances};

const USAGE: &str = "\
usage: audio-diff <reference> <test> [options]

Compares a test render (e.g. the Rust port) against a reference render (the
C++ harness). Inputs: .wav (PCM or float) or .npy ([N] or [N, C]).

Tolerances (a metric is only checked when its limit is given):
  --max-abs-error X       max |test - ref| over all samples
  --rms-diff-db X         max |level difference| in dB
  --residual-db X         max null-test residual, dB relative to reference RMS
                          (e.g. -80 means the difference is 80 dB down)
  --lsd-db X              max mean log-spectral distance in dB
  --tolerances FILE       JSON object with any of the keys above
                          (max_abs_error, rms_diff_db, residual_db, lsd_db);
                          explicit flags override the file

Analysis options:
  --fft-size N            STFT size for the spectral metric (default 2048)
  --hop N                 STFT hop (default 512)
  --silence-db X          skip STFT frames quieter than this in both (default -100)
  --bands-per-octave N    spectral metric band resolution (default 6)
  --dynamic-range-db X    spectral metric range below each frame's peak (default 80)
  --allow-length-mismatch compare the common prefix (still reported as a failure)

Output:
  --json FILE             write the full report as JSON ('-' for stdout)
  --quiet                 no human-readable summary

Exit status: 0 pass, 1 fail, 2 usage or input error.
";

struct Cli {
    reference: PathBuf,
    test: PathBuf,
    tolerances: Tolerances,
    tolerances_file: Option<PathBuf>,
    options: Options,
    json: Option<String>,
    quiet: bool,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut positional = Vec::new();
    let mut tolerances = Tolerances::default();
    let mut tolerances_file = None;
    let mut options = Options::default();
    let mut json = None;
    let mut quiet = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let mut value = || -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("missing value for {arg}"))
        };
        let number = |s: String| s.parse::<f64>().map_err(|_| format!("bad number '{s}'"));
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--max-abs-error" => tolerances.max_abs_error = Some(number(value()?)?),
            "--rms-diff-db" => tolerances.rms_diff_db = Some(number(value()?)?),
            "--residual-db" => tolerances.residual_db = Some(number(value()?)?),
            "--lsd-db" => tolerances.lsd_db = Some(number(value()?)?),
            "--tolerances" => tolerances_file = Some(PathBuf::from(value()?)),
            "--fft-size" => options.fft_size = number(value()?)? as usize,
            "--hop" => options.hop = number(value()?)? as usize,
            "--silence-db" => options.silence_db = number(value()?)?,
            "--bands-per-octave" => options.bands_per_octave = number(value()?)? as usize,
            "--dynamic-range-db" => options.dynamic_range_db = number(value()?)?,
            "--allow-length-mismatch" => options.allow_length_mismatch = true,
            "--json" => json = Some(value()?),
            "--quiet" => quiet = true,
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => positional.push(PathBuf::from(arg)),
        }
        i += 1;
    }

    if positional.len() != 2 {
        return Err("expected exactly two inputs: <reference> <test>".into());
    }
    let test = positional.pop().unwrap();
    let reference = positional.pop().unwrap();
    Ok(Cli {
        reference,
        test,
        tolerances,
        tolerances_file,
        options,
        json,
        quiet,
    })
}

fn fmt_db(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{value:.3}")
    }
}

fn print_summary(cli: &Cli, report: &Report) {
    println!("reference: {}", cli.reference.display());
    println!("test:      {}", cli.test.display());
    println!(
        "frames: ref {} / test {} (compared {}), channels {}, sample rate {}",
        report.frames_ref,
        report.frames_test,
        report.frames_compared,
        report.channels,
        report.sample_rate.map_or("n/a".into(), |r| r.to_string())
    );
    for (c, m) in report.per_channel.iter().enumerate() {
        println!(
            "  ch{c}: max_abs {:.3e} @{}  rms ref {} / test {} dB  rms_diff {} dB  residual {} dB  lsd {} dB ({} frames)",
            m.max_abs_error,
            m.max_abs_error_frame,
            fmt_db(m.rms_ref_db),
            fmt_db(m.rms_test_db),
            fmt_db(m.rms_diff_db),
            fmt_db(m.residual_db),
            fmt_db(m.lsd_db),
            m.lsd_frames
        );
    }
    if report.checks.is_empty() {
        println!("no tolerances given: report only");
    }
    for check in &report.checks {
        let value = if check.metric == "max_abs_error" {
            format!("{:.3e}", check.value)
        } else {
            fmt_db(check.value)
        };
        println!(
            "  [{}] {} = {} (limit {})",
            if check.pass { "pass" } else { "FAIL" },
            check.metric,
            value,
            check.limit
        );
    }
    if report.frames_ref != report.frames_test {
        println!("  [FAIL] length mismatch");
    }
    println!("{}", if report.pass { "PASS" } else { "FAIL" });
}

fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) if message.is_empty() => {
            print!("{USAGE}");
            std::process::exit(0);
        }
        Err(message) => return Err(format!("{message}\n\n{USAGE}")),
    };

    if let Some(path) = &cli.tolerances_file {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let from_file: Tolerances =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        cli.tolerances = cli.tolerances.clone().or(&from_file);
    }

    let reference = load(&cli.reference).map_err(|e| e.to_string())?;
    let test = load(&cli.test).map_err(|e| e.to_string())?;
    let report =
        compare(&reference, &test, &cli.tolerances, &cli.options).map_err(|e| e.to_string())?;

    if !cli.quiet {
        print_summary(&cli, &report);
    }
    if let Some(path) = &cli.json {
        let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        if path == "-" {
            println!("{text}");
        } else {
            std::fs::write(path, text + "\n").map_err(|e| format!("{path}: {e}"))?;
        }
    }
    Ok(report.pass)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("audio-diff: {message}");
            ExitCode::from(2)
        }
    }
}
