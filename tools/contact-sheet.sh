#!/usr/bin/env bash
# Contact sheet of every procedural 3D scene: one row per scene, one column
# per playhead fraction, composed into a single labelled grid image.
#
#   ./tools/contact-sheet.sh [filter] [output.png]
#
# filter: substring match on scene labels (e.g. "combustion"); default all.
# output: default target/contact-sheet/sheet.png.
#
# Requires ImageMagick (magick) and a debug build; builds if missing.
set -euo pipefail
cd "$(dirname "$0")/.."

FILTER="${1:-}"
OUT="${2:-target/contact-sheet/sheet.png}"
BIN=target/debug/chemspec-app
FRACS=(0.22 0.55 0.85)
WORK=target/contact-sheet/frames
mkdir -p "$WORK"
rm -f "$WORK"/*.ppm "$WORK"/*.png "$WORK"/*.meta

cargo build -p chemspec-app --quiet

# label|extra args (every scene gets --structural-3d-smoke + playhead + dump)
SCENES=(
  "alkali-li|--smoke-reaction=alkali-water-lithium"
  "alkali-na|--smoke-reaction=alkali-water-sodium"
  "alkali-k|--smoke-reaction=alkali-water-potassium"
  "explosion-rb|--smoke-reaction=alkali-water-rubidium"
  "explosion-cs|--smoke-reaction=alkali-water-caesium"
  "explosion-fr|--smoke-reaction=alkali-water-francium"
  "precip-agcl|--smoke-reaction=silver-halide-precipitation-chloride"
  "precip-agi|--smoke-reaction=silver-halide-precipitation-iodide"
  "neutralise-nacl|--smoke-reaction=acid-base-sodium-chloride"
  "gas-carbonate|--smoke-reaction=acid-carbonate-sodium-chloride"
  "combustion|--smoke-dynamic=combustion-methane"
  "combustion-inc|--smoke-dynamic=combustion-methane-incomplete"
  "displacement|--smoke-dynamic=displacement-zinc-copper"
  "synthesis|--smoke-dynamic=synthesis-iron-sulfur"
)

render() { # label extra_arg frac index
  local ppm="$WORK/$4-$1@$3.ppm"
  "$BIN" --structural-3d-smoke "$2" "--smoke-playhead-frac=$3" \
    "--dump-frame=$ppm" >/dev/null 2>&1 &
  local pid=$!
  for _ in $(seq 1 60); do # dump fires ~1.2s after launch
    [ -s "$ppm" ] && break
    sleep 0.25
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  if [ ! -s "$ppm" ]; then
    echo "FAILED: $1 @ $3 (no frame dumped)" >&2
    return 1
  fi
  magick "$ppm" -resize 480x "${ppm%.ppm}.png"
  rm -f "$ppm" "${ppm%.ppm}.meta"
}

index=0
for entry in "${SCENES[@]}"; do
  label="${entry%%|*}"
  arg="${entry#*|}"
  index=$((index + 1))
  if [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]]; then continue; fi
  for frac in "${FRACS[@]}"; do
    printf 'rendering %s @ %s\n' "$label" "$frac"
    render "$label" "$arg" "$frac" "$(printf '%02d' "$index")"
  done
done

shopt -s nullglob
frames=("$WORK"/*.png)
[ "${#frames[@]}" -gt 0 ] || { echo "no frames rendered" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
# Nix ImageMagick lacks a fontconfig default; hand it a system font file.
FONT_ARGS=()
[ -f /System/Library/Fonts/Monaco.ttf ] && FONT_ARGS=(-font /System/Library/Fonts/Monaco.ttf)
magick montage -label '%[basename]' "${frames[@]}" \
  -tile "${#FRACS[@]}x" -geometry +4+4 "${FONT_ARGS[@]}" \
  -background '#1c1c20' -fill '#d8d8dc' -pointsize 13 "$OUT"
echo "contact sheet: $OUT (${#frames[@]} frames)"
