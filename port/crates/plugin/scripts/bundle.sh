#!/usr/bin/env bash
# Builds sas-plugin in release mode and wraps it in a VST3 bundle.
#
#   port/crates/plugin/scripts/bundle.sh [OUT_DIR]     (default: build/plugin)
#
# SAS_PLUGIN_FEATURES, if set, is passed to cargo as --features (for example
# focus-spike for the G1 keyboard-focus check build).
#
# macOS: the shipping build, Apple Silicon only (aarch64-apple-darwin), signed
# ad hoc for local use. No universal or x86_64 macOS bundle is made.
# Linux: an x86_64 bundle for running a VST3 validator during development
# only; it is not a shipped product.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
out="${1:-$repo/build/plugin}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

name="synth-a-size"
bundle_id="io.github.salnad.synth-a-size.vst3"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/port/Cargo.toml" | head -1)"
bundle="$out/$name.vst3"
features=()
if [[ -n "${SAS_PLUGIN_FEATURES:-}" ]]; then
  features=(--features "$SAS_PLUGIN_FEATURES")
fi

# The GPLv3 licence and the notices for bundled third-party material
# (the editor's fonts), in Contents/Resources.
copy_notices() {
  local resources="$1/Contents/Resources"
  mkdir -p "$resources"
  cp "$repo/LICENSE" "$resources/LICENSE.txt"
  cp "$here/../notices/THIRD_PARTY_NOTICES.md" "$here/../notices/OFL-1.1.txt" "$resources/"
  cp "$repo/vital/fonts/LICENSE" "$resources/Apache-2.0.txt"
}

case "$(uname -s)" in
  Darwin)
    target=aarch64-apple-darwin
    export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
    rustup target add "$target" >/dev/null 2>&1 || true
    cargo build --release --locked -p sas-plugin --target "$target" ${features[@]+"${features[@]}"} --manifest-path "$repo/port/Cargo.toml"
    library="$repo/port/target/$target/release/libsas_plugin.dylib"

    rm -rf "$bundle"
    mkdir -p "$bundle/Contents/MacOS"
    cp "$library" "$bundle/Contents/MacOS/$name"
    copy_notices "$bundle"
    printf 'BNDL????' > "$bundle/Contents/PkgInfo"
    cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>English</string>
  <key>CFBundleExecutable</key>
  <string>$name</string>
  <key>CFBundleIdentifier</key>
  <string>$bundle_id</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$name</string>
  <key>CFBundleDisplayName</key>
  <string>$name</string>
  <key>CFBundlePackageType</key>
  <string>BNDL</string>
  <key>CFBundleSignature</key>
  <string>????</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>NSHumanReadableCopyright</key>
  <string>synth-a-size contributors, GPL-3.0-only</string>
</dict>
</plist>
PLIST
    plutil -lint "$bundle/Contents/Info.plist"

    archs="$(lipo -archs "$bundle/Contents/MacOS/$name")"
    if [[ "$archs" != "arm64" ]]; then
      echo "expected an arm64-only binary, found: $archs" >&2
      exit 1
    fi

    codesign --force --sign - --timestamp=none "$bundle"
    codesign --verify --strict --verbose=2 "$bundle"
    ;;
  Linux)
    cargo build --release --locked -p sas-plugin ${features[@]+"${features[@]}"} --manifest-path "$repo/port/Cargo.toml"
    rm -rf "$bundle"
    mkdir -p "$bundle/Contents/x86_64-linux"
    cp "$repo/port/target/release/libsas_plugin.so" "$bundle/Contents/x86_64-linux/$name.so"
    copy_notices "$bundle"
    ;;
  *)
    echo "unsupported platform $(uname -s)" >&2
    exit 1
    ;;
esac

echo "$bundle"
