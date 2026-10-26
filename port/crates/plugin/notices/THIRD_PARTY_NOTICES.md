# Third-party notices

synth-a-size is free software under the GNU General Public License,
version 3 (`LICENSE.txt`). It includes code translated from JUCE under
version 3 only, so the plugin as a whole is distributed under version 3
only. Source files without such code are also available under version 3
or later. It is a port of an open-source GPLv3 synthesizer and bundles the
following third-party material.

## Fonts

The editor embeds five font files, unmodified, as the upstream synthesizer's
source distributes them:

| Font | Copyright | Licence |
|---|---|---|
| Lato Regular, Lato Light | Copyright (c) 2010-2011 by tyPoland Lukasz Dziedzic (http://www.typoland.com/), with Reserved Font Name "Lato" | SIL Open Font License 1.1 (`OFL-1.1.txt`) |
| Montserrat Light, Montserrat Regular | Copyright 2011 The Montserrat Project Authors (https://github.com/JulietaUla/Montserrat) | SIL Open Font License 1.1 (`OFL-1.1.txt`) |
| Droid Sans Mono | Digitized data copyright © 2007, Google Corporation | Apache License 2.0 (`Apache-2.0.txt`) |

## Code translated from C++ libraries

Parts of the plugin's source are translations of functions from two C++
libraries that the upstream synthesizer's source distributes. Each such
source file names what it translates and carries the copyright notice.

### JUCE

JUCE 6.0.5, Copyright (c) 2020 - Raw Material Software Limited. JUCE is
available under commercial or open-source licences; synth-a-size uses it
as follows:

- `juce_dsp` (the FFT), `juce_graphics` (colours, paths and path
  strokes), `juce_gui_basics` (slider dragging, click and double-click
  timing, buttons, colour lookup) and `juce_data_structures` (the config
  file location), under the GNU General Public License, version 3 only;
- `juce_core` (base64, string and file-name helpers) and
  `juce_audio_basics` (on-screen keyboard state, MIDI file reading, MPE
  zones, MIDI event ordering, flush-to-zero), under the ISC licence:

> Permission to use, copy, modify, and/or distribute this software for any
> purpose with or without fee is hereby granted provided that the above
> copyright notice and this permission notice appear in all copies.
>
> JUCE IS PROVIDED "AS IS" WITHOUT ANY WARRANTY, AND ALL WARRANTIES, WHETHER
> EXPRESSED OR IMPLIED, INCLUDING MERCHANTABILITY AND FITNESS FOR PURPOSE, ARE
> DISCLAIMED.

### JSON for Modern C++

nlohmann/json 3.1.2 (its double-to-string conversion, which implements
Florian Loitsch's Grisu2), Copyright (c) 2013-2018 Niels Lohmann
<http://nlohmann.me>, under the MIT licence:

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## Rust crates

The plugin links Rust crates under permissive licences (MIT, Apache-2.0,
ISC, Zlib, BSD, Unicode-3.0, Unlicense and similar), all compatible with
the GPLv3, and the audio decoders below under the Mozilla Public License
2.0. Where a crate offers a choice, the permissive option applies.
Their licences are in each crate's source on crates.io. One, egui-baseview 0.9.0 (MIT OR
Apache-2.0, Copyright (c) 2021 Billy Messenger), is built from a copy with
small local changes, under its Apache-2.0 option (the licence text is
`Apache-2.0.txt` in this bundle). The changed copy, its licence files and a
list of the changes are in the synth-a-size source under
`port/vendor/egui-baseview`.

## Audio decoders

The plugin reads AIFF, FLAC, Ogg Vorbis, MP3 and AAC/M4A files dropped on
a wavetable view with Symphonia 0.6.1 (https://github.com/pdeljanov/Symphonia,
Copyright (c) 2019-2026 The Project Symphonia Developers), used unmodified:
the crates `symphonia`, `symphonia-core`, `symphonia-common`,
`symphonia-metadata`, `symphonia-bundle-flac`, `symphonia-bundle-mp3`,
`symphonia-codec-aac`, `symphonia-codec-pcm`, `symphonia-codec-vorbis`,
`symphonia-format-isomp4`, `symphonia-format-ogg` and
`symphonia-format-riff`. They are under the Mozilla Public License 2.0
(`MPL-2.0.txt` in this bundle), which is compatible with the GPLv3. Their
source code is available from crates.io and the repository above.
