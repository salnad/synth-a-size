## 2026-10-27: GUI quads rebuild their coordinates from the pixel centre

**Changed.** The quad shader (`primitives/quad.wgsl`) now computes each
fragment's shader coordinates (upstream's `coordinates_out`) from the pixel
centre, measured from the quad's centre, instead of using the value
interpolated from the vertices.

- **Why.** Upstream renders each component in its own viewport
  (`OpenGlComponent`), so a quad's vertices sit exactly on its edges. On a
  quad an odd number of pixels wide, the centre column interpolates to
  exactly x = 0. The port draws every quad in one viewport per egui paint
  callback, with vertex positions in normalized device coordinates that
  carry rounding. The centre column then lands a hair below zero.
- **Where it shows.** The shaders that take `atan(x, -y)` put that column on
  the other side of the angle's split: −π instead of π. On a bipolar
  modulation amount knob with a negative value (LFO 1's −0.4 in the
  `modulations` view), upstream fills the one-pixel column above the
  knob's centre with the arc colour and the port didn't. Found while
  re-checking #153's and #155's gallery pairs against #129's live engine.
- **How.**
  - Each instance also carries its rect in viewport pixels (left, bottom,
    width, height).
  - The fragment shader snaps the interpolated pixel offset to the pixel
    centre, which is exact because egui starts a callback's viewport on a
    whole pixel (`ViewportInPixels` rounds).
  - It then measures the offset from the quad's centre, so the centre
    column gives exactly +0, as upstream's does.
- **Ruled out first.**
  - The triangle diagonal: a strip sharing upstream's diagonal changed
    nothing.
  - The vertex order: upstream's exact list, `0, 1, 2` and `2, 3, 0`,
    changed nothing.
  - The knob geometry: it is identical to the pixel.

**Verified.**

- **Gallery against ref-gui, before and after.** Rendered locally at
  `main`'s `dddb1e9` plus #120, with ref-gui built from the same tree. The
  columns count differing pixels, first with any channel more than 24 apart
  and then with any difference at all.

  | View | > 24, before | > 24, after | Any, before | Any, after |
  |---|---:|---:|---:|---:|
  | `all_effects` | 24509 | 24509 | 126549 | 126498 |
  | `all_effects_bottom` | 24057 | 24057 | 124529 | 124500 |
  | `filters_analog_comb` | 20820 | 20820 | 107412 | 107395 |
  | `filters_digital_diode` | 20826 | 20826 | 107354 | 107338 |
  | `filters_dirty_phaser` | 20691 | 20691 | 107234 | 107218 |
  | `filters_formant_ladder` | 20819 | 20819 | 107322 | 107307 |
  | `init_advanced` | 25490 | 25490 | 116352 | 116341 |
  | `init_effects` | 13997 | 13997 | 71015 | 71006 |
  | `init_matrix` | 56130 | 56130 | 307697 | 307688 |
  | `init_voice` | 17618 | 17618 | 99292 | 99272 |
  | `modulations` | 17421 | 17414 | 98935 | 98901 |
  | `modulations_many` | 17842 | 17842 | 99368 | 99347 |
  | `sample_on` | 18954 | 18954 | 104715 | 104699 |
  | total | 299174 | 299167 | 1577774 | 1577510 |

  - Most of what remains is text antialiasing, upstream's branding and
    its not-yet-ported parts, none of which this touches.
  - Pixel by pixel, each view has 25 to 81 pixels nearer upstream and 17
    to 36 further away. Every one that moved away did so by exactly one
    colour step: rounding around the now exact value.
  - The `modulations` knob's centre column now matches upstream.
- **Test.** `a_knobs_centre_column_is_on_the_right_wherever_it_sits`
  (`tests/gpu.rs`, `gpu-tests`) draws thirty 41 px modulation knobs across
  a 1400 px viewport and checks that each one's centre column takes the
  right-hand colour. It fails on `main` and passes here.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings`, and with
  `--features gpu-tests`, pass, as do `cargo test --release -p sas-gui
  --features gpu-tests`, the `sas-plugin` tests and `gui_reference.py
  layout --check`.
