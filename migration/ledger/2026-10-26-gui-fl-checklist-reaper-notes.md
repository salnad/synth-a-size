## 2026-10-26: FL Studio checklist, REAPER as the interim host

**Changed.** Salil's correction: FL Studio on his arm64 Mac stays the
target host, and the FL checks and FL-specific behaviour stay. REAPER is
only his test host until FL Studio is installed.

`decisions/gui-framework.md` now names REAPER as the interim host, and each
FL Studio checklist item has a REAPER note saying whether the check can run
there and what a REAPER answer settles:

- Keyboard focus (G1) runs in REAPER, with its "Send all keyboard input to
  plug-in" option off and on, but only answers for REAPER.
- Knob drag speed (G2, D15) is about upstream and display scale, not the
  host, so a REAPER side-by-side settles it, but only at display scale 2:
  REAPER must not be set to "Open in Low Resolution", and both editors'
  text must be crisp. Otherwise upstream's `getDisplayScale()` reads the
  window's backing scale as 1, both sides measure in points, and the check
  settles nothing.
- A learned knob with the editor open (D16) gives an interim answer. The
  echo, the automation curve and undo steps are the host's, so all three
  are repeated in FL Studio.
- The chat's Keychain prompt (L5) only answers for REAPER, because
  Keychain grants access to the host app that asks.

**Verified.** Docs only; no code changes.
