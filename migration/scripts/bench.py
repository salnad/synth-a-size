#!/usr/bin/env python3
"""Engine performance benchmark: the Rust port against the C++ reference.

Builds (unless --no-build) the official reference, its no-fast-math variant
and the port's sas-bench in release mode, then times every scenario in
migration/corpus/bench/scenarios.json at every sample rate and block size
with each engine, interleaved, on this machine. Writes results.json and
report.md to --out (default build/bench) and prints the report. How to read
it: migration/BENCHMARKS.md.

    python3 migration/scripts/bench.py                  # everything
    python3 migration/scripts/bench.py --quick          # 48 kHz / 256 only, short renders
    python3 migration/scripts/bench.py --only heavy_fx --engines rust,cpp

Python 3.8+ standard library only.
"""

import argparse
import array
import datetime
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
CORPUS = os.path.join(REPO, "migration", "corpus")
SCENARIOS = os.path.join(CORPUS, "bench", "scenarios.json")

ENGINES = {
    "rust": "Rust port (sas-bench)",
    "cpp": "C++ reference (official, fast-math)",
    "cpp-nofastmath": "C++ reference (no fast-math)",
}
# Rust output within this residual of the official reference counts as the
# same audio, i.e. the timing compares like for like. Parity tolerances are
# far stricter; this only catches an engine doing different work.
SAME_AUDIO_DB = -40.0


def log(*args):
    print(*args, file=sys.stderr, flush=True)


def default_paths():
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "port", "target")
    exe = ".exe" if os.name == "nt" else ""
    return {
        "rust": os.path.join(target, "release", "sas-bench" + exe),
        "cpp": os.path.join(REPO, "migration", "reference", "build", "ref-bench"),
        "cpp-nofastmath": os.path.join(REPO, "migration", "reference", "build-nofastmath", "ref-bench"),
    }


def build(engines):
    script = os.path.join(HERE, "build_reference.sh")
    if "cpp" in engines:
        log("building the official reference (migration/reference/build)")
        subprocess.run([script], check=True, cwd=REPO, stdout=sys.stderr)
    if "cpp-nofastmath" in engines:
        log("building the no-fast-math reference (migration/reference/build-nofastmath)")
        env = dict(os.environ, REF_BUILD_DIR=os.path.join(REPO, "migration", "reference", "build-nofastmath"))
        subprocess.run([script, "-DREF_FAST_MATH=OFF"], check=True, cwd=REPO, env=env, stdout=sys.stderr)
    if "rust" in engines:
        log("building sas-bench (release)")
        subprocess.run(["cargo", "build", "--release", "--locked", "-p", "sas-bench",
                        "--manifest-path", os.path.join(REPO, "port", "Cargo.toml")],
                       check=True, cwd=REPO, stdout=sys.stderr)


def command_output(command, cwd=REPO):
    try:
        return subprocess.run(command, cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def machine_info():
    cpu = None
    if sys.platform == "darwin":
        cpu = command_output(["sysctl", "-n", "machdep.cpu.brand_string"])
    elif os.path.exists("/proc/cpuinfo"):
        with open("/proc/cpuinfo") as handle:
            for line in handle:
                if line.startswith("model name"):
                    cpu = line.split(":", 1)[1].strip()
                    break
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "rustc": command_output(["rustc", "--version"], cwd=os.path.join(REPO, "port")),
        "github_runner": os.environ.get("RUNNER_NAME"),
        "date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "commit": command_output(["git", "rev-parse", "HEAD"]),
    }


def read_wav_f32(path):
    """Interleaved samples of a 32-bit float WAV (as both harnesses write)."""
    with open(path, "rb") as handle:
        data = handle.read()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("%s: not a WAV file" % path)
    position = 12
    while position + 8 <= len(data):
        chunk, size = data[position:position + 4], int.from_bytes(data[position + 4:position + 8], "little")
        if chunk == b"data":
            samples = array.array("f")
            samples.frombytes(data[position + 8:position + 8 + size])
            if sys.byteorder != "little":
                samples.byteswap()
            return samples
        position += 8 + size + (size & 1)
    raise ValueError("%s: no data chunk" % path)


def residual_db(test, reference):
    """Residual of `test` against `reference` in dB (RMS of the difference
    over RMS of the reference), as audio-diff's residual_db."""
    if len(test) != len(reference):
        return None
    diff = sum((a - b) * (a - b) for a, b in zip(test, reference))
    ref = sum(b * b for b in reference)
    if diff == 0.0:
        return -math.inf
    if ref == 0.0:
        return math.inf
    return 10.0 * math.log10(diff / ref)


def summarize(raw, sample_rate, block_size):
    walls = [r["wall_seconds"] for r in raw["repeats"]]
    cpus = [r["cpu_seconds"] for r in raw["repeats"]]
    wall = statistics.median(walls)
    cpu = statistics.median(cpus)
    blocks = raw["blocks"]
    cpu_us_per_block = 1e6 * cpu / blocks
    block_budget_us = 1e6 * block_size / sample_rate
    return {
        "realtime_factor": raw["audio_seconds"] / wall,
        "wall_seconds": wall,
        "wall_seconds_min": min(walls),
        "wall_seconds_max": max(walls),
        "cpu_seconds": cpu,
        "cpu_us_per_block": cpu_us_per_block,
        "budget_percent": 100.0 * cpu_us_per_block / block_budget_us,
        "block_us_p50": raw["block_us"]["p50"],
        "block_us_p99": raw["block_us"]["p99"],
        "block_us_max": raw["block_us"]["max"],
        "block_us_first": raw["block_us"].get("first"),
    }


def run_engine(binary, scenario, sample_rate, block_size, settings, wav):
    command = [binary]
    command += ["--init"] if scenario["preset"] is None else ["--preset", scenario["preset"]]
    command += ["--midi", scenario["midi"]] if scenario.get("midi") else ["--no-midi"]
    command += [
        "--sample-rate", str(sample_rate),
        "--block-size", str(block_size),
        "--length", repr(settings["length"]),
        "--bpm", repr(settings["bpm"]),
        "--seed", str(settings["seed"]),
        "--warmup", str(settings["warmup"]),
        "--repeats", str(settings["repeats"]),
        "--out", wav,
    ]
    result = subprocess.run(command, cwd=CORPUS, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError("%s exited %d: %s" % (os.path.basename(binary), result.returncode,
                                                 result.stderr.strip()[-500:]))
    return json.loads(result.stdout)


def fmt(value, digits=1):
    if value is None:
        return "–"
    if isinstance(value, float) and math.isinf(value):
        return "−∞" if value < 0 else "∞"
    return "%.*f" % (digits, value)


def ratio(rows, a, b):
    if a in rows and b in rows:
        return rows[a]["summary"]["wall_seconds"] / rows[b]["summary"]["wall_seconds"]
    return None


def match_text(config):
    value = config.get("rust_vs_cpp_db")
    if value is None:
        return "–"
    verdict = "same" if value <= SAME_AUDIO_DB else "**differs**"
    text = "%s (%s dB)" % (verdict, fmt(value))
    if config.get("rust_bitexact_nofastmath"):
        text += ", bit-exact with no-fast-math"
    return text


def report(results):
    machine = results["machine"]
    settings = results["settings"]
    engines = results["engines"]
    lines = ["# Engine benchmark: Rust port vs C++ reference", ""]
    lines.append("- **Machine:** %s; %s; %s logical CPUs." % (machine["cpu"] or machine["machine"],
                                                             machine["platform"], machine["logical_cpus"]))
    lines.append("- **Commit:** `%s`; %s." % ((machine["commit"] or "?")[:12], machine["date"]))
    for engine in engines:
        build_info = results["builds"].get(engine) or {}
        if engine == "rust":
            detail = "%s, %s profile" % (machine["rustc"], build_info.get("profile", "?"))
        else:
            detail = "compiler %s, `%s`" % (build_info.get("compiler", "?"), build_info.get("opt_flags", "?"))
        lines.append("- **%s:** %s." % (ENGINES[engine], detail))
    lines.append("- **Method:** median of %d timed renders of %.1f s after %d untimed, per configuration; "
                 "engines interleaved." % (settings["repeats"], settings["length"], settings["warmup"]))
    lines.append("- **×RT** (realtime factor) = audio seconds rendered per second of wall-clock time; "
                 "higher is faster. **Rust ÷ C++** = Rust render time over C++ render time; above 1 means "
                 "the port is slower. **Output** compares the Rust render with the official reference "
                 "(\"same\" at or below %.0f dB residual), and says when it is bit-exact with the "
                 "no-fast-math build (equal output hashes)." % SAME_AUDIO_DB)
    lines.append("")

    configs = results["configs"]
    headline = results["headline"]
    head = [c for c in configs if c["sample_rate"] == headline["sample_rate"] and c["block_size"] == headline["block_size"]]
    if not head:
        head = configs
        lines.append("## Summary (first configuration of each scenario)")
    else:
        lines.append("## Headline (%d Hz, %d-sample blocks)" % (headline["sample_rate"], headline["block_size"]))
    lines.append("")
    seen = set()
    lines.append("| Scenario | Rust ×RT | C++ ×RT | C++ no-fast-math ×RT | Rust ÷ C++ | Rust ÷ C++ no-fast-math | Output | Note |")
    lines.append("|---|--:|--:|--:|--:|--:|---|---|")
    for config in head:
        if config["scenario"] in seen:
            continue
        seen.add(config["scenario"])
        rows = config["engines"]
        rtf = lambda e: fmt(rows[e]["summary"]["realtime_factor"]) if e in rows else "–"
        note = config.get("needs") or ""
        lines.append("| `%s` | %s | %s | %s | %s | %s | %s | %s |" % (
            config["scenario"], rtf("rust"), rtf("cpp"), rtf("cpp-nofastmath"),
            fmt(ratio(rows, "rust", "cpp"), 2), fmt(ratio(rows, "rust", "cpp-nofastmath"), 2),
            match_text(config), "needs " + note if note else ""))
    lines.append("")

    lines.append("## Every configuration")
    lines.append("")
    lines.append("CPU µs/block is the thread CPU time of the timed block loop divided by the number of blocks; "
                 "% budget is that over the block's real-time duration. p99 and max are wall-clock times of "
                 "single blocks across all timed repeats; first is the first block (every note-on), "
                 "averaged over the repeats.")
    lines.append("")
    lines.append("| Scenario | Hz | Block | Engine | ×RT | CPU µs/block | % budget | p99 µs | max µs | first µs | Rust ÷ engine |")
    lines.append("|---|--:|--:|---|--:|--:|--:|--:|--:|--:|--:|")
    for config in configs:
        for engine in engines:
            row = config["engines"].get(engine)
            if row is None:
                continue
            s = row["summary"]
            lines.append("| `%s` | %d | %d | %s | %s | %s | %s | %s | %s | %s | %s |" % (
                config["scenario"], config["sample_rate"], config["block_size"], engine,
                fmt(s["realtime_factor"]), fmt(s["cpu_us_per_block"]), fmt(s["budget_percent"], 2),
                fmt(s["block_us_p99"]), fmt(s["block_us_max"]), fmt(s["block_us_first"]),
                "" if engine == "rust" else fmt(ratio(config["engines"], "rust", engine), 2)))
    if results["errors"]:
        lines += ["", "## Errors", ""]
        lines += ["- %s" % error for error in results["errors"]]
    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", default=os.path.join(REPO, "build", "bench"), help="output directory (default build/bench)")
    parser.add_argument("--engines", default="rust,cpp,cpp-nofastmath",
                        help="comma-separated subset of: " + ", ".join(ENGINES))
    parser.add_argument("--only", action="append", default=[], help="scenario name (repeatable)")
    parser.add_argument("--rates", help="comma-separated sample rates (default: scenarios.json)")
    parser.add_argument("--blocks", help="comma-separated block sizes (default: scenarios.json)")
    parser.add_argument("--length", type=float, help="seconds per render")
    parser.add_argument("--repeats", type=int, help="timed renders per configuration")
    parser.add_argument("--warmup", type=int, help="untimed renders per configuration")
    parser.add_argument("--quick", action="store_true",
                        help="headline rate and block size only, 2 s renders, 1 repeat (a smoke run)")
    parser.add_argument("--no-build", action="store_true", help="use the binaries as built")
    parser.add_argument("--list", action="store_true", help="list the scenarios and exit")
    for engine in ENGINES:
        parser.add_argument("--%s-bin" % engine, help="path to the %s binary" % engine)
    args = parser.parse_args()

    with open(SCENARIOS) as handle:
        spec = json.load(handle)
    scenarios = spec["scenarios"]
    if args.list:
        for scenario in scenarios:
            print("%-20s %s" % (scenario["name"], scenario["description"]))
        return 0
    names = {s["name"] for s in scenarios}
    unknown = [name for name in args.only if name not in names]
    if unknown:
        parser.error("unknown scenario(s): %s (see --list)" % ", ".join(unknown))
    if args.only:
        scenarios = [s for s in scenarios if s["name"] in args.only]
    engines = [e.strip() for e in args.engines.split(",") if e.strip()]
    if not engines or any(e not in ENGINES for e in engines):
        parser.error("--engines takes a comma-separated subset of: " + ", ".join(ENGINES))

    settings = dict(spec["defaults"])
    headline = spec["headline"]
    if args.quick:
        settings.update(length=2.0, repeats=1, warmup=1,
                        sample_rates=[headline["sample_rate"]], block_sizes=[headline["block_size"]])
    if args.rates:
        settings["sample_rates"] = [int(v) for v in args.rates.split(",")]
    if args.blocks:
        settings["block_sizes"] = [int(v) for v in args.blocks.split(",")]
    for key in ("length", "repeats", "warmup"):
        if getattr(args, key) is not None:
            settings[key] = getattr(args, key)

    paths = default_paths()
    for engine in ENGINES:
        override = getattr(args, "%s_bin" % engine.replace("-", "_"))
        if override:
            paths[engine] = os.path.abspath(override)
    if not args.no_build:
        build(engines)
    for engine in engines:
        if not os.path.exists(paths[engine]):
            parser.error("missing %s binary %s (build it, or drop --no-build)" % (engine, paths[engine]))

    os.makedirs(args.out, exist_ok=True)
    results = {
        "machine": machine_info(),
        "settings": settings,
        "headline": headline,
        "engines": engines,
        "binaries": {e: paths[e] for e in engines},
        "builds": {},
        "configs": [],
        "errors": [],
    }
    wav_dir = tempfile.mkdtemp(prefix="sas-bench-")
    total = len(scenarios) * len(settings["sample_rates"]) * len(settings["block_sizes"])
    index = 0
    try:
        for scenario in scenarios:
            for sample_rate in settings["sample_rates"]:
                for block_size in settings["block_sizes"]:
                    index += 1
                    config = {
                        "scenario": scenario["name"],
                        "sample_rate": sample_rate,
                        "block_size": block_size,
                        "engines": {},
                    }
                    if scenario.get("needs"):
                        config["needs"] = scenario["needs"]
                    # Rotate the engine order so no engine always runs first.
                    order = engines[index % len(engines):] + engines[:index % len(engines)]
                    wavs = {}
                    for engine in order:
                        wav = os.path.join(wav_dir, engine + ".wav")
                        try:
                            raw = run_engine(paths[engine], scenario, sample_rate, block_size, settings, wav)
                        except (RuntimeError, ValueError) as error:
                            message = "%s %d Hz/%d %s: %s" % (scenario["name"], sample_rate, block_size, engine, error)
                            log("error:", message)
                            results["errors"].append(message)
                            continue
                        results["builds"].setdefault(engine, raw.get("build"))
                        raw.pop("build", None)
                        config["engines"][engine] = {"raw": raw, "summary": summarize(raw, sample_rate, block_size)}
                        wavs[engine] = wav
                    if "rust" in wavs and "cpp" in wavs:
                        config["rust_vs_cpp_db"] = residual_db(read_wav_f32(wavs["rust"]), read_wav_f32(wavs["cpp"]))
                    if "rust" in config["engines"] and "cpp-nofastmath" in config["engines"]:
                        hashes = [config["engines"][e]["raw"]["output"]["samples_fnv1a64"] for e in ("rust", "cpp-nofastmath")]
                        config["rust_bitexact_nofastmath"] = hashes[0] == hashes[1]
                    if "cpp-nofastmath" in wavs and "cpp" in wavs:
                        config["nofastmath_vs_cpp_db"] = residual_db(read_wav_f32(wavs["cpp-nofastmath"]),
                                                                     read_wav_f32(wavs["cpp"]))
                    results["configs"].append(config)
                    log("[%d/%d] %s %d Hz/%d: %s" % (index, total, scenario["name"], sample_rate, block_size, ", ".join(
                        "%s %s×RT" % (e, fmt(config["engines"][e]["summary"]["realtime_factor"]))
                        for e in engines if e in config["engines"])))
    finally:
        shutil.rmtree(wav_dir, ignore_errors=True)

    def finite(value):
        if isinstance(value, float) and not math.isfinite(value):
            return str(value)
        if isinstance(value, dict):
            return {k: finite(v) for k, v in value.items()}
        if isinstance(value, list):
            return [finite(v) for v in value]
        return value

    with open(os.path.join(args.out, "results.json"), "w") as handle:
        json.dump(finite(results), handle, indent=2)
        handle.write("\n")
    text = report(results)
    with open(os.path.join(args.out, "report.md"), "w") as handle:
        handle.write(text)
    print(text)
    log("wrote %s and %s" % (os.path.join(args.out, "results.json"), os.path.join(args.out, "report.md")))
    return 1 if results["errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
