#!/usr/bin/env bash
# Build the game and launcher from a clean clone.
#
#   tools/build.sh [mac|windows|linux] [debug|release]
#
# Output goes to dist/<platform>-<profile>/ (binaries + content).
#   debug    dev profile with Bevy dynamic linking, hot reload, debug UI
#   release  optimized, no debug UI; the "dist" profile (fat LTO) is used
#            when DIST=1 is set (shipping builds)
#
# Environment:
#   FETCH_ASSETS=1   run tools/fetch_assets.py first (needs network)
#   CHECK=1          run a `cargo check` pass first (fast error feedback; off by
#                    default because on a clean clone it compiles deps twice)
#   DIST=1           use the fat-LTO shipping profile for release
#
# Windows builds are produced in CI (windows-latest). Locally they can be
# cross-compiled with cargo-xwin: `cargo install cargo-xwin` and run with
# platform "windows" on a Mac/Linux host.
set -euo pipefail

PLATFORM="${1:-}"
PROFILE="${2:-debug}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

if [[ -z "$PLATFORM" ]]; then
  case "$(uname -s)" in
    Darwin) PLATFORM=mac ;;
    Linux) PLATFORM=linux ;;
    MINGW* | MSYS* | CYGWIN*) PLATFORM=windows ;;
    *) usage ;;
  esac
fi

case "$PLATFORM" in
  mac) TARGET="aarch64-apple-darwin" ;;
  windows) TARGET="x86_64-pc-windows-msvc" ;;
  linux) TARGET="x86_64-unknown-linux-gnu" ;;
  *) usage ;;
esac
case "$PROFILE" in
  debug | release) ;;
  *) usage ;;
esac

HOST_OS="$(uname -s)"
CARGO=(cargo)
EXE=""
if [[ "$PLATFORM" == "windows" ]]; then
  EXE=".exe"
  if [[ "$HOST_OS" != MINGW* && "$HOST_OS" != MSYS* && "$HOST_OS" != CYGWIN* ]]; then
    if ! command -v cargo-xwin >/dev/null; then
      echo "Windows builds run in CI. For a local cross-compile install cargo-xwin:" >&2
      echo "  cargo install cargo-xwin && rustup target add $TARGET" >&2
      exit 1
    fi
    CARGO=(cargo xwin)
  fi
fi
if [[ "$PLATFORM" == "mac" && "$HOST_OS" != "Darwin" ]]; then
  echo "macOS builds must run on macOS (Apple SDK and signing tools)." >&2
  exit 1
fi

HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
TARGET_ARGS=()
CROSS=0
if [[ "$TARGET" != "$HOST_TRIPLE" ]]; then
  CROSS=1
  # Cross build: separate target dir. Native builds share target/<profile>
  # with plain `cargo build`/`cargo dev` so nothing is compiled twice.
  rustup target add "$TARGET" >/dev/null 2>&1 || true
  TARGET_ARGS=(--target "$TARGET")
fi

GAME_FEATURES=()
CARGO_PROFILE_ARGS=()
OUT_PROFILE_DIR="debug"
if [[ "$PROFILE" == "debug" ]]; then
  # Dynamic linking is a dev-only speedup; it is never shipped.
  GAME_FEATURES=(--features dev)
else
  if [[ "${DIST:-0}" == "1" ]]; then
    CARGO_PROFILE_ARGS=(--profile dist)
    OUT_PROFILE_DIR="dist"
  else
    CARGO_PROFILE_ARGS=(--release)
    OUT_PROFILE_DIR="release"
  fi
fi

if [[ "${FETCH_ASSETS:-0}" == "1" ]]; then
  python3 tools/fetch_assets.py || echo "asset fetch failed; continuing with procedural fallbacks"
fi

echo "==> $PLATFORM/$PROFILE ($TARGET)"
if [[ "${CHECK:-0}" == "1" ]]; then
  "${CARGO[@]}" check ${TARGET_ARGS[@]+"${TARGET_ARGS[@]}"} -p game ${GAME_FEATURES[@]+"${GAME_FEATURES[@]}"} ${CARGO_PROFILE_ARGS[@]+"${CARGO_PROFILE_ARGS[@]}"}
fi
"${CARGO[@]}" build ${TARGET_ARGS[@]+"${TARGET_ARGS[@]}"} -p game ${GAME_FEATURES[@]+"${GAME_FEATURES[@]}"} ${CARGO_PROFILE_ARGS[@]+"${CARGO_PROFILE_ARGS[@]}"}
"${CARGO[@]}" build ${TARGET_ARGS[@]+"${TARGET_ARGS[@]}"} -p launcher ${CARGO_PROFILE_ARGS[@]+"${CARGO_PROFILE_ARGS[@]}"}

if [[ "$CROSS" == "1" ]]; then
  BIN_DIR="target/$TARGET/$OUT_PROFILE_DIR"
else
  BIN_DIR="target/$OUT_PROFILE_DIR"
fi
OUT="dist/$PLATFORM-$PROFILE"
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$BIN_DIR/game$EXE" "$BIN_DIR/launcher$EXE" "$OUT/"
if [[ "$PROFILE" == "debug" ]]; then
  # Dynamic-linking builds need the Bevy dylib (and std) next to the binary.
  # Copy exactly the Bevy dylib the game binary references (target/ may hold
  # others from different feature sets). The name is stored as a string in
  # ELF, Mach-O and PE import tables alike.
  DYLIB_NAME="$(LC_ALL=C grep -aoE '(lib)?bevy_dylib-[0-9a-f]+\.(so|dylib|dll)' "$BIN_DIR/game$EXE" | head -n 1 || true)"
  if [[ -n "$DYLIB_NAME" && -f "$BIN_DIR/deps/$DYLIB_NAME" ]]; then
    cp "$BIN_DIR/deps/$DYLIB_NAME" "$OUT/"
  else
    echo "warning: could not locate the Bevy dylib for $BIN_DIR/game$EXE" >&2
  fi
  SYSROOT="$(rustc --print sysroot)"
  find "$SYSROOT/lib/rustlib/$TARGET/lib" -maxdepth 1 \( -name 'libstd-*.dylib' -o -name 'libstd-*.so' -o -name 'std-*.dll' \) \
    -exec cp {} "$OUT/" \; 2>/dev/null || true
fi
# Content: data, shaders, and assets (excluding the download cache).
cp -R data shaders "$OUT/"
mkdir -p "$OUT/assets"
(cd assets && find . -path ./.cache -prune -o -type f -print) | while read -r f; do
  mkdir -p "$OUT/assets/$(dirname "$f")"
  cp "assets/$f" "$OUT/assets/$f"
done
cp CREDITS.md "$OUT/" 2>/dev/null || true

echo "==> built $OUT"
du -sh "$OUT" | awk '{print "    size: " $1}'
echo "    run:  $OUT/launcher$EXE"
if [[ "$PROFILE" == "debug" ]]; then
  echo "    (debug builds link Bevy dynamically: start the game through the launcher,"
  echo "     or use 'cargo dev' from the repo)"
fi
