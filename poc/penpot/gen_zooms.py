import subprocess, sys, shutil
from pathlib import Path
HERE = Path(__file__).resolve().parent
STILLS = HERE / 'stills'
CLIPS = HERE / 'clips'
SEG = Path('/tmp/vinyl-poc-zooms')
FRAMES = 120
BG='0x0C0B09'
BASE=(f'scale=2880:1800:force_original_aspect_ratio=decrease:flags=lanczos,pad=2880:1800:(ow-iw)/2:(oh-ih)/2:color={BG},setsar=1')
CENTER_X="iw/2-(iw/zoom/2)"; CENTER_Y="ih/2-(ih/zoom/2)"
ZOOMS={
 'zoom-01-tokens-colors-zoom-in':('01-tokens--colors-dark.png',f"z='1+0.5*on/{FRAMES-1}':x='{CENTER_X}':y='{CENTER_Y}'"),
 'zoom-02-type-pan-right':('01-tokens--type.png',f"z='1.4':x='(iw-iw/zoom)*on/{FRAMES-1}':y='{CENTER_Y}'"),
 'zoom-03-library-zoom-in':('03-screens-library--library-desktop-dark-paused.png',f"z='1+0.5*on/{FRAMES-1}':x='{CENTER_X}':y='{CENTER_Y}'"),
 'zoom-04-nowplaying-zoom-out':('03-screens-nowplaying--library-nowplaying-desktop-dark.png',f"z='1.5-0.5*on/{FRAMES-1}':x='{CENTER_X}':y='{CENTER_Y}'"),
 'zoom-05-mobile-pan-down':('03-screens-mobile--mobile-390-dark.png',f"z='1.4':x='{CENTER_X}':y='(ih-ih/zoom)*on/{FRAMES-1}'"),
 'zoom-06-library-open-pan-right':('00-penpot-library-open.png',f"z='1.4':x='(iw-iw/zoom)*on/{FRAMES-1}':y='{CENTER_Y}'"),
}
def run(cmd):
 r=subprocess.run(cmd,capture_output=True,text=True)
 if r.returncode: print(r.stderr[-1500:]); sys.exit(1)
if SEG.exists(): shutil.rmtree(SEG)
SEG.mkdir(parents=True)
for name,(fr,zp) in ZOOMS.items():
 src=STILLS/fr
 assert src.exists(), src
 vf=f'{BASE},zoompan={zp}:d={FRAMES}:s=1440x900:fps=30,fade=t=in:st=0:d=0.3,fade=t=out:st=3.7:d=0.3,format=yuv420p'
 mp4=CLIPS/f'{name}.mp4'
 run(['ffmpeg','-y','-loop','1','-i',str(src),'-vf',vf,'-t','4','-c:v','libx264','-pix_fmt','yuv420p',str(mp4)])
 pal=SEG/'pal.png'
 run(['ffmpeg','-y','-i',str(mp4),'-vf','fps=8,scale=720:-1:flags=lanczos,palettegen',str(pal)])
 run(['ffmpeg','-y','-i',str(mp4),'-i',str(pal),'-lavfi','fps=8,scale=720:-1:flags=lanczos [x]; [x][1:v] paletteuse',str(CLIPS/f'{name}.gif')])
 print('zoom done:',name,fr)
print('ALL DONE')
