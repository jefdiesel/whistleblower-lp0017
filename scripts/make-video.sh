#!/usr/bin/env bash
# Produce the LP-0017 captioned screencast: record the REAL on-chain flow on the
# mini (RISC0_DEV_MODE=0), render each scene with agg, overlay a TikTok-style
# caption box per scene, and concatenate into a vertical 1080x1920 mp4.
#
# Run on the build mini (has the sequencer, asciinema, agg, ffmpeg, ImageMagick).
#   SCENES="all"  bash scripts/make-video.sh      # everything + concat
#   SCENES="3"    bash scripts/make-video.sh      # render one scene only (test)
set -uo pipefail
export PATH="/opt/homebrew/bin:$PATH"

# --- real on-chain environment ------------------------------------------------
export RISC0_DEV_MODE=0
export WB_SEQUENCER_URL=http://localhost:3040
export WB_PROGRAM_ID="736378292,3237769127,3218962078,1003268346,3355061132,654317770,4171522436,2002532608"
export WB_SIGNER_KEY="0101010101010101010101010101010101010101010101010101010101010101"
export DEMO="/Users/minim4/lamda/crates/wb-lez-registry/target/debug/batch-demo"
export SAMPLE="/Users/minim4/lamda/scripts/sample-doc.txt"

# --- render settings ----------------------------------------------------------
WIN=110x18          # terminal cols x rows (wide => tx hashes fit on one line)
FONT=23             # agg font size (px)
CANVAS_W=1920; CANVAS_H=1080      # landscape 16:9
FIT_W=1840; FIT_H=760            # terminal fits within this box, aspect preserved
BG=0x0d1117
SCENES="${SCENES:-all}"

CAPFONT=""
for f in "/System/Library/Fonts/Supplemental/Arial Bold.ttf" \
         "/System/Library/Fonts/Supplemental/Arial.ttf" \
         "/Library/Fonts/Arial Unicode.ttf" \
         "/System/Library/Fonts/SFNS.ttf"; do
  [ -f "$f" ] && CAPFONT="$f" && break
done
echo "caption font: $CAPFONT"

WORK=/Users/minim4/wbvideo
mkdir -p "$WORK"; cd "$WORK"

# --- caption box: $1=out.png  $2=text (auto-wrapped, translucent dark box) -----
mkcap() {
  local out="$1" text="$2"
  magick -background "#111827ee" -fill white -font "$CAPFONT" -pointsize 46 \
    -size 1320x -gravity center caption:"$text" \
    -bordercolor "#111827ee" -border 34x22 "$WORK/_boxed.png"
  local h; h=$(magick identify -format "%h" "$WORK/_boxed.png")
  magick -size ${CANVAS_W}x${h} xc:none "$WORK/_boxed.png" -gravity center -composite "$out"
}

# --- footer bar (rendered once, reused on every scene) ------------------------
mkfooter() {
  magick -background "#0b1220cc" -fill "#c9d1d9" -font "$CAPFONT" -pointsize 30 \
    -size 1320x -gravity center \
    caption:"RISC0_DEV_MODE=0    ·    real on-chain    ·    github.com/jefdiesel/whistleblower-lp0017" \
    -bordercolor "#0b1220cc" -border 26x16 "$WORK/_fbox.png"
  local h; h=$(magick identify -format "%h" "$WORK/_fbox.png")
  magick -size ${CANVAS_W}x${h} xc:none "$WORK/_fbox.png" -gravity center -composite "$WORK/footer.png"
}

# --- write the per-scene command scripts (run live; env inherited) ------------
write_scenes() {
cat > s1.sh <<'EOF'
echo "Whistleblower  —  LP-0017"; sleep 0.6
echo "Censorship-resistant document registry on the Logos stack"; sleep 0.8
echo; sleep 0.2
echo "Real on-chain. Real RISC0 zkVM execution (no dev shortcut):"; sleep 0.6
echo "  RISC0_DEV_MODE = $RISC0_DEV_MODE"; sleep 0.6
echo "  LEZ program image id (on-chain address):"; sleep 0.4
echo "    736378292,3237769127, ... ,2002532608"; sleep 2
EOF

cat > s2.sh <<'EOF'
echo "1)  Upload a document to Logos Storage (Codex)"; sleep 0.8
CID=$(curl -sS -X POST http://127.0.0.1:8080/api/storage/v1/data \
  -H "content-type: application/octet-stream" --data-binary @"$SAMPLE")
echo "    -> content CID = $CID"; sleep 2
EOF

cat > s3.sh <<'EOF'
echo "2)  Anchor 1 CID on-chain  (real RISC0_DEV_MODE=0 execution)"
echo
$DEMO 1 vidS$RANDOM
sleep 1
EOF

cat > s4.sh <<'EOF'
echo "3)  Batch-anchor 12 CIDs in ONE transaction"
echo
$DEMO 12 vidB$RANDOM
sleep 1
EOF

cat > s5.sh <<'EOF'
echo "4)  Batch-anchor 50 CIDs in one tx  (the LP-0017 target)"
echo
$DEMO 50 vidF$RANDOM
sleep 1
EOF

cat > s6.sh <<'EOF'
echo "5)  Real executor times from the sequencer log (RISC0_DEV_MODE=0):"; sleep 0.8
echo; sleep 0.2
grep -oE "execution time: [0-9.]+ms" /Users/minim4/seq.log | tail -8; sleep 1
echo; sleep 0.2
echo "    ~3 ms for 1 CID   vs   ~48 ms for 50 CIDs   =   0.97 ms / CID"; sleep 0.8
echo "    batching is ~3x cheaper per CID"; sleep 2
EOF

cat > s7.sh <<'EOF'
echo "Whistleblower  —  LP-0017   (complete)"; sleep 0.6
echo; sleep 0.2
echo "  * Reusable document-indexing module (Rust)"; sleep 0.4
echo "  * On-chain LEZ CID registry  (SPEL / RISC0 zkVM)"; sleep 0.4
echo "  * Permissionless, resumable batch-anchor CLI"; sleep 0.4
echo "  * Query-by-CID  +  idempotent re-anchor"; sleep 0.4
echo "  * Dual-licensed  MIT OR Apache-2.0"; sleep 0.6
echo; sleep 0.2
echo "  github.com/jefdiesel/whistleblower-lp0017"; sleep 2
EOF
}

# scene captions (index matches s<N>.sh)
CAP1="Censorship-resistant document registry on Logos — real on-chain"
CAP2="1 · Upload to Logos Storage → content CID"
CAP3="2 · Anchor a CID on-chain (RISC0_DEV_MODE=0)"
CAP4="3 · Batch-anchor 12 CIDs in ONE transaction"
CAP5="4 · 50 CIDs in one tx — every record verified on-chain"
CAP6="5 · ~3ms (1 CID) vs ~48ms (50) = 0.97ms/CID"
CAP7="github.com/jefdiesel/whistleblower-lp0017 · MIT OR Apache-2.0"

render_one() {
  local n="$1"; local capvar="CAP$n"; local cap="${!capvar}"
  echo "=== scene $n ==="
  asciinema rec --overwrite --window-size "$WIN" -c "bash $WORK/s$n.sh" "$WORK/scene$n.cast" >/dev/null 2>&1
  agg --idle-time-limit 1 --last-frame-duration 2.5 --font-size "$FONT" "$WORK/scene$n.cast" "$WORK/scene$n.gif" 2>/dev/null
  mkcap "$WORK/cap$n.png" "$cap"
  # Pre-compose caption + footer into ONE 8-bit full-frame overlay. A single
  # overlay avoids ffmpeg choking on chained 16-bit (Q16) PNG overlays.
  magick -size ${CANVAS_W}x${CANVAS_H} xc:none \
    "$WORK/cap$n.png" -gravity north -geometry +0+30 -composite \
    "$WORK/footer.png" -gravity south -geometry +0+30 -composite \
    -depth 8 PNG32:"$WORK/frame$n.png"
  # fps + setsar up front normalize the gif's irregular timing so libx264 accepts it.
  ffmpeg -y -i "$WORK/scene$n.gif" -i "$WORK/frame$n.png" -filter_complex \
    "[0:v]fps=30,scale=${FIT_W}:${FIT_H}:force_original_aspect_ratio=decrease:flags=lanczos,setsar=1[t];[t]pad=${CANVAS_W}:${CANVAS_H}:(ow-iw)/2:(oh-ih)/2:color=${BG}[p];[p][1:v]overlay=0:0,format=yuv420p[v]" \
    -map "[v]" -an -r 30 -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p "$WORK/scene$n.mp4" >/dev/null 2>&1 \
    && echo "  scene$n.mp4 OK ($(du -h "$WORK/scene$n.mp4" | cut -f1))" || echo "  scene$n FFMPEG FAILED"
}

write_scenes
mkfooter

if [ "$SCENES" = "all" ]; then
  : > "$WORK/list.txt"
  for n in 1 2 3 4 5 6 7; do
    render_one "$n"
    echo "file 'scene$n.mp4'" >> "$WORK/list.txt"
  done
  echo "=== concatenating ==="
  ffmpeg -y -f concat -safe 0 -i "$WORK/list.txt" \
    -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -movflags +faststart \
    "$WORK/whistleblower-lp0017-demo.mp4" >/dev/null 2>&1 \
    && echo "FINAL: $WORK/whistleblower-lp0017-demo.mp4 ($(du -h "$WORK/whistleblower-lp0017-demo.mp4" | cut -f1))" \
    || echo "CONCAT FAILED"
  ffprobe -v error -select_streams v:0 -show_entries stream=width,height,duration,nb_frames \
    -of default=noprint_wrappers=1 "$WORK/whistleblower-lp0017-demo.mp4" 2>/dev/null
else
  render_one "$SCENES"
fi
