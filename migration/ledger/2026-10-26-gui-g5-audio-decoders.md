## 2026-10-26: GUI G5 (5), upstream's audio formats for the wavetable drop

**Changed.** The wavetable view now takes upstream's macOS audio formats,
not only WAV. D23 narrows to CAF and the rarer CoreAudio types, and the
lossy-decoder note.

- **Formats:** `.wav`, `.bwf`, `.aif`, `.aiff`, `.flac`, `.ogg`, `.mp3`,
  `.m4a` and `.aac` (`audio_file::AUDIO_EXTENSIONS`). These are the
  wildcards of JUCE's `registerBasicFormats()` on macOS, without CAF and the
  rarer CoreAudio types.
- **Decoding** (`audio_file::read_audio`):
  - WAV keeps the port's own reader, which Graph Reviewer found
    sample-exact against JUCE in #151. It now also matches JUCE on
    malformed files: an extensible `fmt ` chunk shorter than 40 bytes, or
    with a sub-format GUID other than PCM, float or their ambisonic forms,
    is refused, and a `data` chunk longer than the file reads its declared
    frames with zeros past the end.
  - The other formats decode with Symphonia 0.6.1 (MPL-2.0), with features
    `pcm`, `wav`, `aiff`, `flac`, `ogg`, `vorbis`, `mp3`, `aac` and
    `isomp4` only. The first channel is kept, and integer samples convert
    to float by exact powers of two, as JUCE's readers do. FLAC samples
    arrive left-justified.
- **Bundle:** `THIRD_PARTY_NOTICES.md` names the twelve Symphonia crates
  and their licence, and the licence statement now says the decoders are
  MPL-2.0. `MPL-2.0.txt` is added, and `bundle.sh` copies it.
- **Reference and corpus** (for Parity Reviewer):
  - `generate.py` authors 11 audio files in `migration/corpus/audio` with
    the standard library: WAV variants, AIFF and FLAC. The FLAC writer
    uses verbatim subframes with CRC-8 and CRC-16, and the AIFF writer an
    80-bit sample rate.
  - New artifact-only `ref-module` module `audio_file_decode`
    (`migration/reference/src/modules/audio_file/decode.cpp`) reads a
    corpus file as `WavetableEditSection::loadAudioFile` does (JUCE's
    `AudioFormatManager`, `registerBasicFormats()`, read as float). It
    emits the first channel and the sample rate.
  - Case file `corpus/modules/audio_file/decode.json` has one case per
    file, at `max_abs_error` 0. Hashes are written for the x86_64 CI
    identity; the macOS identity follows from its advisory job.
- **Lossy formats** (Ogg Vorbis, MP3, AAC/M4A): no fixtures, per the
  coordinator. Committing encoder output would stretch hard rule 3. They
  rely on Symphonia's own codec tests, and D23 says their samples can
  differ slightly from JUCE's Ogg reader and CoreAudio's decoders.

**Verified.**

- `tests/audio_reference.rs`: all 11 authored files decode in the port to
  the reference's JUCE samples exactly (zero tolerance) and the same
  sample rates.
- `tests/audio_files.rs`:
  - FLAC and AIFF give exactly the WAV reader's samples for the same
    tones;
  - every authored file decodes its first channel, and the truncated one
    zero-fills;
  - every offered extension refuses an empty and a corrupt file;
  - the offered set is upstream's macOS set without CAF.
- `audio_file` unit tests: the new extensible and truncated-data cases.
- sas-plugin, `a_dropped_wav_becomes_the_oscillators_wavetable`: an empty
  and a corrupt file of every offered type, lossy ones included, are
  refused with the revision unchanged.
- `generate.py --check`, `test_run_corpus.py`, `check_names.sh`,
  `verify_upstream.sh`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo fmt`, and the sas-plugin and sas-gui tests pass.

**Host checks.** Checklist item 5 now drags a WAV and then an MP3 or M4A
(FL Studio, with the REAPER note).
