# Third-party notices

synth-a-size is free software under the GNU General Public License,
version 3 or later (`LICENSE.txt`). It is a port of an open-source GPLv3
synthesizer and bundles the following third-party material.

## Fonts

The editor embeds five font files, unmodified, as the upstream synthesizer's
source distributes them:

| Font | Copyright | Licence |
|---|---|---|
| Lato Regular, Lato Light | Copyright (c) 2010-2011 by tyPoland Lukasz Dziedzic (http://www.typoland.com/), with Reserved Font Name "Lato" | SIL Open Font License 1.1 (`OFL-1.1.txt`) |
| Montserrat Light, Montserrat Regular | Copyright 2011 The Montserrat Project Authors (https://github.com/JulietaUla/Montserrat) | SIL Open Font License 1.1 (`OFL-1.1.txt`) |
| Droid Sans Mono | Digitized data copyright © 2007, Google Corporation | Apache License 2.0 (`Apache-2.0.txt`) |

## Rust crates

The plugin links Rust crates under permissive licences (MIT, Apache-2.0,
ISC, Zlib, BSD, Unlicense and similar), all compatible with the GPLv3. Where
a crate offers a choice, the permissive option applies. Their licences are in
each crate's source on crates.io. One, egui-baseview 0.9.0 (MIT OR
Apache-2.0, Copyright (c) 2021 Billy Messenger), is built from a copy with
small local changes; that copy and its licence files are in the
synth-a-size source under `port/vendor/egui-baseview`.
