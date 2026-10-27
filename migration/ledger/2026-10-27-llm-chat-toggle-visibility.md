## 2026-10-27: LLM follow-up, the chat toggle shows only with our logo

**Why.** Found by Wavetable Builder. With the wavetable editor open (it
replaces the interface, as upstream's `showWavetableEditSection` does), the
chat toggle stayed at the logo's position and floated over the editor's
empty top-left.

**Upstream, at the logo's position**
- The wavetable editor: `showWavetableEditSection` hides the whole header,
  logo included.
- The preset browser: `setPresetBrowserVisibility` keeps the header (it
  sets the tab selector's temporary "PRESET BROWSER" tab), so the logo
  shows.
- The save section and the delete section: `Overlay`s over the whole
  interface (`setBounds(bounds)`). The logo shows dimmed beneath, and a
  click outside the section's box closes it (`SaveSection::mouseUp`), at
  the logo too.

**Changed**

- `sas-gui`:
  - `Interface::logo_shown`: the logo shows and takes clicks. That means
    no wavetable editor replaces the header, and nothing that closes on a
    click outside it covers the logo (the save and delete sections, listed
    in one `covered` array so more can be added).
    `ChatPlacement::logo_shown` carries it.
  - `ChatPanel` draws its toggle only while the logo shows, so a click
    there reaches the wavetable editor or closes the save section, as
    upstream.
  - The drawer's open state doesn't change while the toggle is away. A
    docked panel stays, since the drawer is beside the editor and not part
    of upstream's interface. A panel overlaying the editor (the host kept
    the window's width) hides with the logo and comes back, still open,
    when the logo does, so it never covers the wavetable editor or sits
    above the save section's dimming.
- `migration/DIVERGENCES.md` D13 says when the toggle shows.

**Measured**

- `cargo test -p sas-gui --test controls`: 101 tests, 3 new, on the editor
  composed with the chat as `App::ui` draws it:
  - with the drawer docked, the wavetable editor hides the toggle and the
    docked chat stays. A click where the logo was leaves the drawer open,
    and after the editor closes, the toggle is back and closes the drawer;
  - an overlaid chat hides with the logo, still open, and comes back;
  - with the save section open, the toggle is hidden, and a click at the
    logo closes the save section without opening the drawer.
  - Drawing the toggle unconditionally fails all three.
- `cargo test -p sas-gui --test preset_selector`: 1 new test. The preset
  browser keeps the logo; the delete section (opened from a row's menu)
  hides it; closing the section brings it back. Leaving the delete section
  out of `covered` fails it.
- Clippy `-D warnings` for `sas-gui` and `sas-plugin` and rustfmt are clean.
  All `sas-gui` tests pass.

**Review (#177).** UI Reviewer approved at 29fc025, with a mutation check of
the three composed tests. Its nit, a delete-section test, is above. Its
follow-up for #172's popup browser, whose closing areas cover the logo and
close the browser on a click there (`closingAreaClicked`), is left to
whichever of #172 and this merges second: add the popup browser to
`covered`, with a test like the save section's.

**Blocked:** nothing.
