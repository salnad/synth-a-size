## 2026-10-21: M6 effects, parked cases re-enabled as no-fast-math exact (SAS-35)

**Changed.** Every parked effects case from the M6 PRs is back in its module file, unchanged, with `"reference": "nofastmath"`. Each is listed in `migration/decisions/nofastmath/effects/<module>.md`, and `migration/corpus/modules/effects/pending/` is gone. That makes 69 cases:

- **x86-justified (59):** 10 reverb, 3 reverb module, 2 distortion, 1 distortion module, 16 chorus, 8 flanger, 13 multiband compressor and 6 compressor module cases.
- **macOS-only (10):** 2 stereo delay, 1 distortion and 7 flanger cases. These were the arm64 parks that had also dropped the cases from x86. Declared `macos`, they are now compared exactly against the no-fast-math build on both targets, so x86 has them again too.

**x86 justification** (`compare build/parity build/proxy --justify-nofastmath`): all 59 `x86` cases print `justified on x86`, and the 10 `macos` cases print `not gated on x86`. 2475 compared, 0 failed.

**Port exactness:** on x86 the port is bit-identical to the no-fast-math captures on all 69 cases (`SAS_PARITY_CAPTURES_NOFASTMATH`, sas-effects parity tests).

**Hashes:** x86 hashes for both builds come from local CI-identity trees. `hashes --check --require-known-identity` covers 2620 hashes and the no-fast-math check covers 272, both with 0 problems.

**macOS:** Reference ARM is non-gating by owner policy. Run 39664460173 (macOS 15, on the first 50 cases) prints `justified on macos` for exactly the 10 `macos` declarations, and `not gated on macos` for the 40 `x86` ones, with 0 `NOT justified`. The macOS official and no-fast-math hashes for those 50 cases come from that run (`migration/scripts/macos_hashes.sh 39664460173`, hash-only commit). The 19 compressor cases are `x86` only. Their macOS hashes are added from this PR's next macOS run if it arrives; missing macOS hashes are a warning.

`allowlist-approval --base origin/main` lists the 69 additions with 0 problems.

Official vs no-fast-math, worst output per case (max abs / residual). x86 values are from this change; macOS values are from the earlier Reference ARM artifacts (runs 39622357646 and 39622362623):

| Case | Targets | Official vs no-fast-math |
|---|---|---|
| `effects/chorus_module/defaults_saw` | x86 | 1.5e−02 / −76.4 dB |
| `effects/chorus_module/voices_1` | x86 | 2.4e−02 / −63.6 dB |
| `effects/chorus_module/voices_2` | x86 | 5.9e−02 / −52.2 dB |
| `effects/chorus_module/voices_3` | x86 | 7.1e−02 / −51.0 dB |
| `effects/chorus_module/voices_sweep` | x86 | 1.1e−02 / −75.8 dB |
| `effects/chorus_module/free_frequency_sweep` | x86 | 4.7e−02 / −65.3 dB |
| `effects/chorus_module/tempo_sweep` | x86 | 3.3e−03 / −85.9 dB |
| `effects/chorus_module/dotted_and_triplet` | x86 | 7.0e−02 / −55.8 dB |
| `effects/chorus_module/feedback_and_depth_sweep` | x86 | 4.5e−03 / −88.9 dB |
| `effects/chorus_module/delay_time_sweeps` | x86 | 6.4e−02 / −56.4 dB |
| `effects/chorus_module/filter_and_wet_sweep` | x86 | 1.0e−01 / −52.5 dB |
| `effects/chorus_module/enable_toggle` | x86 | 2.4e−03 / −89.6 dB |
| `effects/chorus_module/rate_48k` | x86 | 1.1e−03 / −90.8 dB |
| `effects/chorus_module/rate_96k` | x86 | 1.4e−02 / −84.1 dB |
| `effects/chorus_module/oversampled_2x` | x86 | 3.4e−03 / −87.6 dB |
| `effects/chorus_module/long_tail_max_feedback` | x86 | 1.9e−04 / −69.6 dB |
| `effects/distortion/down_sample_drive_sweep_48k` | x86 | 5.1e−02 / −62.3 dB |
| `effects/distortion/down_sample_96k` | x86 | 9.3e−01 / −7.3 dB |
| `effects/distortion/interim_down_sample_drive_sweep_short` | macos | 1.8e−01 / −44.1 dB |
| `effects/distortion_module/filter_before` | x86 | 2.7e−04 / −88.5 dB |
| `effects/flanger_module/defaults_saw` | x86 | 1.2e−04 / −108.8 dB |
| `effects/flanger_module/free_frequency_sweep` | x86 | 2.9e−04 / −94.1 dB |
| `effects/flanger_module/dotted_and_triplet` | x86 | 2.7e−01 / −43.8 dB |
| `effects/flanger_module/center_sweep` | x86 | 1.7e−03 / −95.1 dB |
| `effects/flanger_module/feedback_sweep` | x86 | 2.9e−04 / −103.2 dB |
| `effects/flanger_module/depth_and_phase_offset_sweep` | x86 | 1.2e−04 / −95.3 dB |
| `effects/flanger_module/enable_toggle` | x86 | 1.9e−04 / −105.7 dB |
| `effects/flanger_module/long_tail_full_feedback` | x86 | 1.7e−05 / −75.2 dB |
| `effects/flanger_module/tempo_sweep` | macos | 3.3e−04 / −98.7 dB |
| `effects/flanger_module/wet_sweep` | macos | 1.6e−04 / −104.8 dB |
| `effects/flanger_module/oversampled_2x` | macos | 4.5e−04 / −94.9 dB |
| `effects/flanger_module/interim_no_depth_enable_toggle` | macos | 1.7e−04 / −102.5 dB |
| `effects/flanger_module/interim_no_depth_phase_offset` | macos | 6.8e−04 / −85.7 dB |
| `effects/flanger_module/interim_small_depth_free_frequency` | macos | 1.3e−04 / −96.7 dB |
| `effects/flanger_module/interim_full_feedback_short_tail` | macos | 3.1e−05 / −74.6 dB |
| `effects/reverb/chorus_sweep_sine` | x86 | 1.9e−04 / −81.0 dB |
| `effects/reverb/decay_sweep_saw` | x86 | 2.0e−04 / −82.3 dB |
| `effects/reverb/filter_sweeps` | x86 | 2.5e−04 / −74.9 dB |
| `effects/reverb/impulse_long_tail` | x86 | 3.2e−06 / −71.1 dB |
| `effects/reverb/max_decay_full_size_tail` | x86 | 1.4e−05 / −57.6 dB |
| `effects/reverb/pre_delay_sweep` | x86 | 1.2e−02 / −61.4 dB |
| `effects/reverb/rate_48k` | x86 | 7.7e−06 / −70.1 dB |
| `effects/reverb/rate_96k` | x86 | 1.0e−02 / −66.1 dB |
| `effects/reverb/rate_192k` | x86 | 3.1e−06 / −72.8 dB |
| `effects/reverb/size_sweep_noise` | x86 | 2.8e−04 / −73.1 dB |
| `effects/reverb_module/decay_and_size_sweep` | x86 | 2.0e−04 / −76.5 dB |
| `effects/reverb_module/filter_and_delay_sweep` | x86 | 3.3e−02 / −45.6 dB |
| `effects/reverb_module/long_tail_max_decay` | x86 | 6.5e−06 / −66.8 dB |
| `effects/stereo_delay/clamped_dampened_damping_sweep` | macos | 3.8e-4 / −92 dB (LEDGER.md, #2) |
| `effects/stereo_delay/unclamped_unfiltered_negative_feedback` | macos | 1.3e-4 / −92 dB (LEDGER.md, #2) |
| `effects/compressor_module/defaults_noise` | x86 | 8.4e−05 / −78.8 dB |
| `effects/compressor_module/threshold_and_ratio_sweeps` | x86 | 5.1e−04 / −76.6 dB |
| `effects/compressor_module/gain_attack_release_mix` | x86 | 1.0e−03 / −71.2 dB |
| `effects/compressor_module/enable_toggle` | x86 | 1.0e−04 / −83.4 dB |
| `effects/compressor_module/rate_96k` | x86 | 3.5e−04 / −64.5 dB |
| `effects/compressor_module/oversampled_2x` | x86 | 3.1e−04 / −65.8 dB |
| `effects/multiband_compressor/multiband_noise` | x86 | 8.4e−05 / −78.8 dB |
| `effects/multiband_compressor/multiband_saw_loud` | x86 | 6.3e−05 / −86.5 dB |
| `effects/multiband_compressor/multiband_quiet_expansion` | x86 | 2.3e−05 / −75.8 dB |
| `effects/multiband_compressor/upper_threshold_sweeps` | x86 | 1.9e−04 / −76.7 dB |
| `effects/multiband_compressor/lower_threshold_sweeps` | x86 | 5.5e−04 / −79.4 dB |
| `effects/multiband_compressor/ratio_sweeps` | x86 | 1.6e−04 / −77.5 dB |
| `effects/multiband_compressor/gain_sweeps` | x86 | 3.7e−04 / −76.5 dB |
| `effects/multiband_compressor/attack_release_sweeps` | x86 | 9.0e−04 / −73.0 dB |
| `effects/multiband_compressor/mix_sweep` | x86 | 3.7e−05 / −91.1 dB |
| `effects/multiband_compressor/band_switching` | x86 | 1.0e−04 / −83.5 dB |
| `effects/multiband_compressor/rate_48k` | x86 | 1.0e−04 / −77.3 dB |
| `effects/multiband_compressor/rate_96k` | x86 | 3.4e−04 / −64.1 dB |
| `effects/multiband_compressor/oversampled_2x` | x86 | 3.6e−04 / −65.8 dB |
