#!/usr/bin/env bash
# Burn TikTok-style captions onto base.mp4 using ImageMagick (text->PNG) + ffmpeg
# overlay (this ffmpeg lacks libfreetype/libass, so drawtext/ass are unavailable).
set -euo pipefail
export PATH=/opt/homebrew/bin:$PATH
cd "$HOME"
FONT="/System/Library/Fonts/Supplemental/Arial Black.ttf"
W=980          # caption text box width (margins inside 1080)
PT=56          # max point size

mkdir -p caps; rm -f caps/*.png

# start | end | text   (timed to the ~30s cast beats; no emoji — Arial Black has no color glyphs)
caps=(
"0.3|3.0|WHISTLEBLOWER"
"3.0|5.5|documents that can't be taken down"
"5.5|9.0|1. upload to Logos Storage  ->  content ID"
"9.0|13.0|2. derive its on-chain account (PDA)"
"13.0|19.0|3. anchor ON-CHAIN  -  RISC0_DEV_MODE=0  -  real proof"
"19.0|24.0|4. query by CID  ->  the record reads back"
"24.0|30.0|permissionless . idempotent . censorship-resistant"
)

inputs=(-i base.mp4)
filter=""; prev="[0:v]"; i=0
for c in "${caps[@]}"; do
  IFS='|' read -r s e t <<< "$c"
  png="caps/c$i.png"
  printf '%s' "$t" | magick -background none -fill white -stroke black -strokewidth 4 \
    -font "$FONT" -pointsize "$PT" -size "${W}x" -gravity center caption:@- "$png"
  inputs+=(-i "$png")
  n=$((i+1))
  # lower band (y center ~85% of height) so captions sit under the terminal, not over it
  filter+="${prev}[${n}:v]overlay=(W-w)/2:H*0.85-h/2:enable='between(t,$s,$e)'[v$i];"
  prev="[v$i]"; i=$((i+1))
done
filter="${filter%;}"

ffmpeg -y "${inputs[@]}" -filter_complex "$filter" -map "${prev}" \
  -pix_fmt yuv420p -movflags +faststart whistleblower_tiktok.mp4
echo "=== done ==="; ls -la whistleblower_tiktok.mp4
ffprobe -v error -show_entries format=duration -show_entries stream=width,height -of default=noprint_wrappers=1 whistleblower_tiktok.mp4
