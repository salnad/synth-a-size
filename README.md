# synth-a-size
the vst of my dreams

A Rust (nice-plug VST3/CLAP) port of an open-source wavetable synthesizer,
with an LLM control layer on top. GPLv3: the plugin as a whole is
distributed under GPL version 3 only, because a few files translate code
from JUCE's GPLv3-only modules. The rest of the source is
GPL-3.0-or-later; each file's SPDX line says which.

| Folder | Contents |
|---|---|
| [`vital/`](vital/) | Unmodified copy of the upstream synth source (GPLv3, original names and README kept), pinned in [`migration/UPSTREAM.md`](migration/UPSTREAM.md). Read-only reference. |
| [`port/`](port/) | The Rust Cargo workspace: SIMD, DSP core, wavetable, oscillators, filters, effects, modulation, engine, presets, plugin, GUI and LLM control crates. |
| [`migration/`](migration/) | Everything used to port and to test the port against the original: C++ reference harness, comparison tool, golden corpus, plan, rules and work log. Start with [`migration/AGENTS.md`](migration/AGENTS.md) and [`migration/PLAN.md`](migration/PLAN.md). |

```bash
cargo build --workspace --manifest-path port/Cargo.toml   # Rust port
migration/scripts/build_reference.sh                      # C++ reference harness (Linux)
```
