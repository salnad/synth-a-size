## 2026-10-22: M8 PR 4, the plugin's init state carries the engine's own white noise (D12 removed)

**Changed.** This closes the temporary divergence D12 from #79.
- `Engine` gains `engine_sample() -> Option<SampleState>`, which defaults to
  `None`. `SoundEngineHost` returns `SoundEngineHost::sample_state()` (#80):
  the white noise the engine generated while it was constructed, with a seed
  from the global counter.
- `SynthPlugin::new` puts that sample in the init document. An untouched
  init state therefore saves the noise the engine plays, as upstream's
  `getStateInformation` saves the engine's own sample.
- Engines that can't report a sample (`SilentEngine`, test engines) keep
  white noise from a local seed. Building the plugin still draws nothing
  from the global seed counter.
- D12 is removed from `migration/DIVERGENCES.md`.

**Measured.** `the_init_state_saves_the_engines_own_white_noise` checks
three things:
- the init document's sample equals `SoundEngineHost::sample_state()`;
- the saved plugin state's `settings.sample` is that sample's JSON;
- `SynthPlugin::new` leaves the seed counter's next value unchanged.

The other checks still pass:
- `the_plugin_plays_the_engines_render` is still bit-identical to the
  engine's offline render;
- 41 `sas-plugin` tests pass;
- workspace clippy is clean.
