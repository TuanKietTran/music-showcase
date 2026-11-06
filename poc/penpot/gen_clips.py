#!/usr/bin/env python3
"""Vinyl Penpot PoC clips: assemble flow clips from real Penpot board exports.
Stills are direct Penpot exports (real captures of the design source).
Clips are slideshows assembled with ffmpeg from those exports, following
the prototype navigate-to links / page order - NOT interactive recordings.
Run: python3 poc/penpot/gen_clips.py  (requires ffmpeg/ffprobe)
"""
import shutil, subprocess, sys
from pathlib import Path

PENPOT_DIR = Path(__file__).resolve().parent
REPO = PENPOT_DIR.parents[1]
SRC = REPO / 'design' / 'penpot' / 'png'
LIB_SHOT = REPO.parent / 'notes' / 'shots' / 'penpot' / 'library-open.png'
STILLS = PENPOT_DIR / 'stills'
CLIPS = PENPOT_DIR / 'clips'
SEG = Path('/tmp/vinyl-poc-segs')
W, H, HOLD, FPS = 1440, 900, 1.8, 30
BG = '0x0C0B09'
VF = (f'scale={W}:{H}:force_original_aspect_ratio=decrease,'
      f'pad={W}:{H}:(ow-iw)/2:(oh-ih)/2:color={BG},setsar=1,fps={FPS},'
      f'fade=t=in:st=0:d=0.3,fade=t=out:st={HOLD-0.3}:d=0.3,format=yuv420p')

FLOWS = {
 'flow-01-unlock-to-nowplaying': [
   '03-screens-tokengate--tokengate-error-desktop-dark.png',
   '03-screens-library--library-desktop-dark-paused.png',
   '03-screens-nowplaying--library-nowplaying-desktop-dark.png'],
 'flow-02-screen-states': [
   '03-screens-loading--loading-desktop-dark.png',
   '03-screens-library-light--library-light-desktop-paused.png',
   '03-screens-empty--empty-desktop-dark.png',
   '03-screens-mobile--mobile-390-dark.png'],
 'flow-03-design-system': [
   '01-tokens--colors-dark.png',
   '01-tokens--type.png',
   '03-screens-flow--flow-prototype-map.png',
   '@LIBRARY@'],
}

def run(cmd):
 r = subprocess.run(cmd, capture_output=True, text=True)
 if r.returncode: print(r.stderr[-1500:]); sys.exit(1)

def main():
 STILLS.mkdir(parents=True, exist_ok=True); CLIPS.mkdir(parents=True, exist_ok=True)
 n = 0
 for f in sorted(SRC.glob('*.png')):
  shutil.copy2(f, STILLS / f.name); n += 1
 shutil.copy2(LIB_SHOT, STILLS / '00-penpot-library-open.png'); n += 1
 print('stills copied:', n)
 if SEG.exists(): shutil.rmtree(SEG)
 SEG.mkdir()
 for name, frames in FLOWS.items():
  segs = []
  for i, fr in enumerate(frames):
   src = LIB_SHOT if fr == '@LIBRARY@' else SRC / fr
   seg = SEG / f'{name}-{i}.mp4'
   run(['ffmpeg','-y','-loop','1','-t',str(HOLD),'-i',str(src),'-vf',VF,'-c:v','libx264','-pix_fmt','yuv420p',str(seg)])
   segs.append(seg)
  lst = SEG / f'{name}.txt'
  lst.write_text(''.join(f"file '{s}'\n" for s in segs))
  mp4 = CLIPS / f'{name}.mp4'
  run(['ffmpeg','-y','-f','concat','-safe','0','-i',str(lst),'-c','copy',str(mp4)])
  pal = SEG / 'pal.png'
  run(['ffmpeg','-y','-i',str(mp4),'-vf','fps=8,scale=720:-1:flags=lanczos,palettegen',str(pal)])
  run(['ffmpeg','-y','-i',str(mp4),'-i',str(pal),'-lavfi','fps=8,scale=720:-1:flags=lanczos [x]; [x][1:v] paletteuse',str(CLIPS / f'{name}.gif')])
  print('clip done:', name)
 print('ALL DONE')

if __name__ == '__main__': main()
