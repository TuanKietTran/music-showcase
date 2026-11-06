from pathlib import Path
from PIL import Image
HERE = Path(__file__).resolve().parent
STILLS = HERE / 'stills'
OUT = HERE / 'details'
OUT.mkdir(parents=True, exist_ok=True)
# name, source still, fractional crop box (x0,y0,x1,y1)
CROPS=[
 ('detail-01-library-top-left.png','03-screens-library--library-desktop-dark-paused.png',(0.0,0.0,0.5,0.5)),
 ('detail-02-library-center.png','03-screens-library--library-desktop-dark-paused.png',(0.25,0.25,0.75,0.75)),
 ('detail-03-nowplaying-center.png','03-screens-nowplaying--library-nowplaying-desktop-dark.png',(0.25,0.2,0.75,0.8)),
 ('detail-04-nowplaying-player-strip.png','03-screens-nowplaying--library-nowplaying-desktop-dark.png',(0.0,0.75,1.0,1.0)),
 ('detail-05-colors-dark-top.png','01-tokens--colors-dark.png',(0.0,0.0,1.0,0.5)),
 ('detail-06-type-left.png','01-tokens--type.png',(0.0,0.0,0.5,1.0)),
 ('detail-07-player-playing-center.png','02-components--player-playing.png',(0.2,0.0,0.8,1.0)),
 ('detail-08-recordcard-playing-center.png','02-components--recordcard-playing.png',(0.125,0.125,0.875,0.875)),
 ('detail-09-mobile-top.png','03-screens-mobile--mobile-390-dark.png',(0.0,0.0,1.0,0.5)),
 ('detail-10-library-open-left-panel.png','00-penpot-library-open.png',(0.0,0.0,0.4,1.0)),
 ('detail-11-flow-map-center.png','03-screens-flow--flow-prototype-map.png',(0.2,0.2,0.8,0.8)),
 ('detail-12-masthead-center.png','02-components--masthead.png',(0.1,0.0,0.9,1.0)),
]
for name,src,(fx0,fy0,fx1,fy1) in CROPS:
 p=STILLS/src
 assert p.exists(), p
 im=Image.open(p); w,h=im.size
 box=(int(w*fx0),int(h*fy0),int(w*fx1),int(h*fy1))
 assert box[2]>box[0] and box[3]>box[1], (name,box)
 im.crop(box).save(OUT/name)
 print('crop done:',name,'src',src,f'{w}x{h}','box',box)
print('ALL DONE', len(CROPS))
