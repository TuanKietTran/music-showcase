import subprocess, sys, shutil
from pathlib import Path
HERE = Path(__file__).resolve().parent
STILLS = HERE / 'stills'
CLIPS = HERE / 'clips'
SEG = Path('/tmp/vinyl-poc-segs')
W,H,HOLD,FPS = 1440,900,1.8,30
BG='0x0C0B09'
VF=(f'scale={W}:{H}:force_original_aspect_ratio=decrease,pad={W}:{H}:(ow-iw)/2:(oh-ih)/2:color={BG},setsar=1,fps={FPS},fade=t=in:st=0:d=0.3,fade=t=out:st={HOLD-0.3}:d=0.3,format=yuv420p')
FLOWS={
 'flow-04-tokens-tour':['01-tokens--colors-dark.png','01-tokens--colors-light.png','01-tokens--type.png','01-tokens--spacing-radii-motion.png'],
 'flow-05-components-player':['02-components--player-bar.png','02-components--player-buffering.png','02-components--player-playing.png','02-components--trackrow-default.png','02-components--trackrow-playing.png'],
 'flow-06-components-cards-buttons':['02-components--button-primary.png','02-components--button-ghost.png','02-components--iconbutton.png','02-components--recordcard.png','02-components--recordcard-disc.png','02-components--recordcard-playing.png','02-components--skeletoncard.png','02-components--errorbanner.png'],
 'flow-07-screens-dark-tour':['03-screens-library--library-desktop-dark-paused.png','03-screens-nowplaying--library-nowplaying-desktop-dark.png','03-screens-flow--flow-prototype-map.png'],
 'flow-08-states-tour':['03-screens-loading--loading-desktop-dark.png','03-screens-empty--empty-desktop-dark.png','03-screens-tokengate--tokengate-error-desktop-dark.png','02-components--tokengate.png','02-components--errorbanner.png'],
 'flow-09-light-mobile-tour':['03-screens-library-light--library-light-desktop-paused.png','01-tokens--colors-light.png','03-screens-mobile--mobile-390-dark.png'],
 'flow-10-full-system-tour':['00-penpot-library-open.png','01-tokens--colors-dark.png','01-tokens--type.png','02-components--masthead.png','02-components--player-playing.png','02-components--recordcard.png','03-screens-library--library-desktop-dark-paused.png','03-screens-nowplaying--library-nowplaying-desktop-dark.png','03-screens-flow--flow-prototype-map.png'],
}
def run(cmd):
 r=subprocess.run(cmd,capture_output=True,text=True)
 if r.returncode: print(r.stderr[-1500:]); sys.exit(1)
if SEG.exists(): shutil.rmtree(SEG)
SEG.mkdir(parents=True)
for name,frames in FLOWS.items():
 segs=[]
 for i,fr in enumerate(frames):
  src=STILLS/fr
  assert src.exists(), src
  seg=SEG/f'{name}-{i}.mp4'
  run(['ffmpeg','-y','-loop','1','-t',str(HOLD),'-i',str(src),'-vf',VF,'-c:v','libx264','-pix_fmt','yuv420p',str(seg)])
  segs.append(seg)
 lst=SEG/f'{name}.txt'; lst.write_text(''.join(f"file '{s}'\n" for s in segs))
 mp4=CLIPS/f'{name}.mp4'
 run(['ffmpeg','-y','-f','concat','-safe','0','-i',str(lst),'-c','copy',str(mp4)])
 pal=SEG/'pal.png'
 run(['ffmpeg','-y','-i',str(mp4),'-vf','fps=8,scale=720:-1:flags=lanczos,palettegen',str(pal)])
 run(['ffmpeg','-y','-i',str(mp4),'-i',str(pal),'-lavfi','fps=8,scale=720:-1:flags=lanczos [x]; [x][1:v] paletteuse',str(CLIPS/f'{name}.gif')])
 print('clip done:',name,len(frames),'frames')
print('ALL DONE')
