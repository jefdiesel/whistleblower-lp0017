#!/usr/bin/env bash
# Mux the recorded narration (narration/001..007.m4a) onto the landscape scene
# visuals (pulled from the mini): normalize/clean each clip, hold each scene's
# final frame for the length of its narration, then concat into the narrated cut.
set -uo pipefail
export PATH="/opt/homebrew/bin:$PATH"

WORK=/tmp/wbnarr; rm -rf "$WORK"; mkdir -p "$WORK"
NARR=/Users/jef/lamda/narration
OUT=/Users/jef/lamda/whistleblower-lp0017-narrated.mp4

echo "== pull scene visuals from mini =="
rsync -a "minim4:/Users/minim4/wbvideo/scene[1-7].mp4" "$WORK/" && echo "got: $(ls "$WORK"/scene*.mp4 | wc -l | tr -d ' ') scenes"

: > "$WORK/list.txt"
for n in 1 2 3 4 5 6 7; do
  a=$(printf "%s/00%d.m4a" "$NARR" "$n")
  v="$WORK/scene$n.mp4"
  [ -f "$a" ] || { echo "MISSING audio $a"; continue; }
  [ -f "$v" ] || { echo "MISSING scene $v"; continue; }

  # clean + normalize voice: de-rumble (highpass), light denoise, loudness to -16 LUFS
  ffmpeg -y -i "$a" \
    -af "highpass=f=80,afftdn=nf=-25,loudnorm=I=-16:TP=-1.5:LRA=11" \
    -ar 48000 -ac 2 "$WORK/a$n.m4a" >/dev/null 2>&1

  # hold the scene's last frame for EXACTLY the narration length. -shortest alone
  # overshoots with a filtergraph (dead air), so cap precisely with -t <audio dur>.
  D=$(ffprobe -v error -show_entries format=duration -of default=nk=1:nw=1 "$WORK/a$n.m4a")
  ffmpeg -y -i "$v" -i "$WORK/a$n.m4a" \
    -filter_complex "[0:v]tpad=stop_mode=clone:stop_duration=180,fps=30,format=yuv420p,setsar=1[v]" \
    -map "[v]" -map 1:a -t "$D" -c:v libx264 -preset medium -crf 20 -c:a aac -b:a 192k \
    -movflags +faststart "$WORK/n$n.mp4" >/dev/null 2>&1 \
    && echo "scene $n muxed -> $(ffprobe -v error -show_entries format=duration -of default=nk=1:nw=1 "$WORK/n$n.mp4" 2>/dev/null)s" \
    || echo "scene $n MUX FAILED"
  echo "file '$WORK/n$n.mp4'" >> "$WORK/list.txt"
done

echo "== concat -> final narrated cut =="
ffmpeg -y -f concat -safe 0 -i "$WORK/list.txt" \
  -c:v libx264 -preset medium -crf 20 -c:a aac -b:a 192k -movflags +faststart "$OUT" >/dev/null 2>&1 \
  && echo "FINAL: $OUT ($(ffprobe -v error -show_entries format=duration -of default=nk=1:nw=1 "$OUT")s)" \
  || echo "CONCAT FAILED"
