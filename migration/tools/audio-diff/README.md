# audio-diff

Compares a test render (the Rust port) with a reference render (the C++
harness) and exits with a pass/fail status. It is written in Rust so
port crates can also use it as a library in their differential tests
(`audio_diff::compare`), for example as a dev-dependency:
`audio-diff = { path = "../../../migration/tools/audio-diff" }`.

```bash
cargo build --release --manifest-path migration/tools/audio-diff/Cargo.toml
migration/tools/audio-diff/target/release/audio-diff ref.wav test.wav \
  --max-abs-error 1e-3 --rms-diff-db 0.1 --residual-db -60 --lsd-db 0.5 --json report.json
```

Inputs are `.wav` (PCM 8/16/24/32-bit or float) or `.npy` (`<f4`, `<f8`,
`<i2`, `<i4`, shape `[N]` or `[N, C]`, as written by `ref-module`). The
channel count must match. Sample rates must match when both are known.
Lengths must match unless `--allow-length-mismatch` is given, which
compares the common prefix but still reports a failure.

## Metrics

Each metric is computed per channel. The reported value is the worst
channel.

| Metric | Definition | Good value |
|---|---|---|
| `max_abs_error` | `max \|test − ref\|`. NaN counts as infinite. | ≈ 0 |
| `rms_diff_db` | `\|20·log10(rms(test) / rms(ref))\|`: overall level mismatch | ≈ 0 dB |
| `residual_db` | `20·log10(rms(test − ref) / rms(ref))`: null-test depth. dBFS if the reference is silent. `-inf` when identical. | as negative as possible |
| `lsd_db` | Band-wise log-spectral distance. STFT with a Hann window (`--fft-size` 2048, `--hop` 512); bin powers are summed into 1/6-octave bands (`--bands-per-octave`). Each signal ignores bands more than 80 dB below its own loudest band in that frame (`--dynamic-range-db`). Frame score = RMS over bands of `10·log10(P_ref/P_test)`. Result = mean over frames that are not silent in both signals (`--silence-db`, default −100 dBFS). A pure gain change of g dB scores exactly \|g\|. | ≈ 0 dB |

`max_abs_error` and `residual_db` are phase-sensitive: they catch
sample-level divergence. `rms_diff_db` and `lsd_db` are phase-blind: they
are the right checks for patches with free-running randomness, where an
exact null cannot be expected.

## Tolerances and exit status

A metric is checked only when its limit is given, as a flag or in a JSON
file passed with `--tolerances` (keys `max_abs_error`, `rms_diff_db`,
`residual_db`, `lsd_db`; flags override the file). Exit status: **0**
all checks pass, **1** a check failed or the lengths differ, **2** usage
or input error (unreadable file, channel or sample-rate mismatch).

`--json FILE` writes the full report: per-channel metrics, the checks,
and the overall result. Non-finite values are written as the strings
`"inf"`, `"-inf"` and `"nan"`.

Per-case tolerances for the golden corpus live in
`migration/corpus/cases.json`. Read `migration/AGENTS.md` before changing
them.

## Tests

`cargo test` covers identical signals, a 6.02 dB gain change on all four
metrics, an octave shift, NaN handling, length mismatch, silent input and
NPY round-trips.
