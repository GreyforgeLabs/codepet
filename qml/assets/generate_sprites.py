#!/usr/bin/env python3
"""Generate CodePet's original MIT-licensed ginger-cat vector animation frames.
No downloaded artwork, fonts, filters, embedded raster images or external references.
"""
from pathlib import Path

OUT = Path(__file__).resolve().parent
INK = '#663e36'

def path(d, fill='none', stroke=INK, width=2.2):
    return f'<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>'

def ellipse(x,y,rx,ry,fill,stroke='none',width=2):
    return f'<ellipse cx="{x}" cy="{y}" rx="{rx}" ry="{ry}" fill="{fill}" stroke="{stroke}" stroke-width="{width}"/>'

def head(frame, closed=False, happy=False, tilt=0, dx=0, dy=0):
    s=f'<g transform="translate({dx} {dy}) rotate({tilt} 62 60)">'
    # Sculpted ears and a ruffled cheek silhouette, rather than a circular head.
    s+=path('M 31,50 Q 25,35 28,20 Q 29,17 33,20 L 46,32 Q 60,27 75,32 L 88,21 Q 92,18 93,23 Q 95,38 89,50 L 96,62 L 90,63 L 93,69 L 85,69 Q 80,81 62,82 Q 43,82 35,71 L 28,70 L 32,64 L 26,62 Z','#efab65')
    s+=path('M 31,26 L 34,43 L 42,35 Z','#db8278','none')+path('M 88,27 L 79,35 L 87,43 Z','#db8278','none')
    s+=path('M 34,46 Q 40,39 46,39 M 80,39 Q 86,40 89,46','#efab65','#f9c88d',3)
    s+=path('M 51,33 L 53,42 M 61,31 L 61,42 M 71,33 L 69,42','none','#bd7348',3)
    s+=path('M 30,55 L 38,57 M 29,61 L 37,62 M 88,56 L 94,54 M 88,62 L 93,60','none','#c17a4c',2.5)
    s+=path('M 43,65 Q 45,58 54,60 Q 61,54 68,60 Q 78,58 82,65 Q 80,77 62,78 Q 46,77 43,65 Z','#fff0d4','none')
    if closed or frame==3:
        s+=path('M 40,54 Q 45,58 50,54 M 73,54 Q 78,58 83,54',width=2.5)
    elif happy:
        s+=path('M 40,55 Q 45,49 50,55 M 73,55 Q 78,49 83,55',width=2.5)
    else:
        for x in [46,77]:
            s+=ellipse(x,54,3.6,5.2,INK)+ellipse(x-1,52.2,1.2,1.5,'#fff9e6')
    s+=ellipse(39,63,4.5,2.4,'#e79179')+ellipse(84,63,4.5,2.4,'#e79179')
    s+=path('M 58,62 Q 62,60 66,62 L 62,66 Z','#a95d59','none')
    s+=path('M 62,66 L 62,69 Q 58,73 55,69 M 62,69 Q 66,73 69,69',width=1.7)
    s+=path('M 40,67 L 29,66 M 40,71 L 31,73 M 82,67 L 95,66 M 82,71 L 92,74','none',INK,1.2)
    return s+'</g>'

def sprite(pose, f):
    s='<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">'
    s+='<title>CodePet ginger cat: '+pose+', frame '+str(f)+'</title>'
    s+=ellipse(63,113,38 if pose!='sleep' else 44,4,'#3b2430',width=0).replace('fill="#3b2430"','fill="#3b2430" opacity=".13"')
    bob=[0,-1,0,1][f] if pose not in ['sleep','bathroom'] else 0
    s+=f'<g transform="translate(0 {bob})">'
    if pose=='sleep':
        s+=path('M 31,100 Q 25,76 45,69 Q 73,57 91,77 Q 106,94 91,108 Q 73,116 48,111 Z','#e9a263')
        s+=path('M 70,73 Q 82,78 85,87 M 61,71 Q 71,78 72,85','none','#c17a4c',4)
        s+=head(f,closed=True,tilt=-18,dx=-13,dy=22)
        s+=path('M 76,91 Q 108,84 103,102 Q 99,119 62,111 Q 51,108 58,101 Q 62,97 76,101 Q 92,105 92,98',' #efab65'.strip(),INK,2.4)
        s+=path('M 62,102 Q 57,106 63,110','none','#fff0d4',7)
        # Sleep marks are vector strokes: no font dependency.
        s+=path('M 94,54 L 102,54 L 94,63 L 102,63 M 105,40 L 115,40 L 105,50 L 115,50','none','#8a9d9e',1.8)
    else:
        tail=[ 'M 83,100 Q 107,104 108,85 Q 108,77 104,74', 'M 83,100 Q 108,103 111,84 Q 113,77 110,73', 'M 83,100 Q 108,103 109,85 Q 109,78 106,75', 'M 83,100 Q 105,106 104,88 Q 104,80 100,77'][f]
        s+=path(tail,'none',INK,12)+path(tail,'none','#df9559',8)
        s+=path('M 102,78 L 104,82','none','#fff0d4',6)
        s+=path('M 45,73 Q 34,85 37,105 Q 39,112 50,111 L 76,111 Q 87,112 88,104 Q 88,83 77,74 Z','#e9a263')
        s+=path('M 53,79 Q 64,74 73,81 Q 82,94 76,105 Q 65,111 50,104 Q 47,92 53,79 Z','#fff0d4','none')
        if pose=='walk':
            shift=[-5,0,5,0][f]
            s+=path(f'M 45,98 Q {39+shift},103 {39+shift},109 Q {40+shift},114 {52+shift},111 L 58,100','#efab65')
            s+=path(f'M 71,100 L {72-shift},110 Q {83-shift},115 {88-shift},110 Q {88-shift},104 81,98','#efab65')
        else:
            s+=path('M 43,99 Q 34,104 36,109 Q 38,114 54,111 L 55,102 M 71,102 L 71,111 Q 88,114 90,109 Q 92,103 82,99','#efab65')
        s+=path('M 42,107 L 42,110 M 48,107 L 48,111 M 78,108 L 78,111 M 84,107 L 84,110','none',INK,1)
        tilt=([0,-3,0,3][f] if pose in ['idle','play'] else 0)
        down=5 if pose in ['food','water'] else 0
        s+=head(f, happy=pose in ['play','wash'], tilt=tilt, dy=down)
        if pose=='play':
            s+=path('M 39,79 Q 27,80 23,91 Q 21,98 27,99 Q 33,99 37,90','#efab65')
            s+=path('M 83,79 Q 91,84 94,90 Q 97,95 91,97 Q 86,99 80,89','#efab65')
            bx=[23,27,32,27][f]
            s+=ellipse(bx,105,11,10,'#91bcb0',INK,2)
            s+=path(f'M {bx-5},97 Q {bx+8},104 {bx-4},113 M {bx-9},103 Q {bx},101 {bx+10},108','none','#d0e4cd',1.7)
            s+=path(f'M {bx+9},110 Q 42,122 52,112','none','#91bcb0',1.5)
        elif pose in ['food','water']:
            s+=path('M 40,84 L 43,100 Q 45,105 50,101 L 51,85 M 73,85 L 74,101 Q 81,105 83,99 L 85,84','#efab65')
            s+=path('M 42,103 L 47,116 Q 63,120 79,116 L 84,103 Z','#7caaa4')
            s+=ellipse(63,103,21,5,'#d7ebe1',INK,2)
            s+=ellipse(63,103,16,2.7,'#aa7152' if pose=='food' else '#85c6d6')
            if pose=='food':
                for x,y in [(54,102),(61,100),(68,103),(74,101)]: s+=ellipse(x,y,2.5,1.6,'#80523a')
            else:
                s+=path('M 53,103 Q 60,101 66,103','none','#e4fcff',1.1)
            if f%2:
                s+=path('M 62,74 L 62,79 Q 65,82 67,78 L 66,75','#e3928f',INK,1)
        elif pose=='wash':
            s+=path('M 41,81 Q 37,87 40,99 M 82,80 Q 90,82 88,94','none',INK,3)
            s+=path('M 31,96 Q 61,102 95,96 L 90,112 Q 63,120 37,112 Z','#9ac6c0')
            s+=path('M 29,95 Q 37,89 44,95 Q 50,85 58,93 Q 65,86 72,93 Q 81,86 87,94 Q 97,90 99,96 Q 97,101 90,100 L 38,101 Q 30,102 29,95 Z','#eefaf1','#729d99',1.8)
            for x,y,r in [(37,88,5),(81,86,4),(99,76+f,4),(25,72-f,3),(57,29-f,4)]:
                s+=ellipse(x,y,r,r,'#d7f1ef','#8bb7b5',1)+ellipse(x-1,y-1,r*.3,r*.3,'#ffffff')
            s+=path('M 49,33 Q 43,27 49,24 Q 53,18 58,24 Q 66,21 68,28 Q 75,27 75,33 Z','#eff9f0','#729d99',1.3)
        elif pose=='bathroom':
            s+=path('M 34,96 L 90,96 L 85,116 L 39,116 Z','#c0a2bf')
            s+=ellipse(62,96,28,5,'#e5d4c3',INK,2)
            s+=path('M 40,83 L 63,89 L 86,83 L 85,108 L 63,113 L 40,107 Z','#fff4dd')
            s+=path('M 63,89 L 63,113','none','#c0a183',1)
            s+=path('M 45,90 L 57,93 M 45,94 L 57,97 M 45,99 L 55,102 M 69,93 L 80,90 M 69,98 L 80,95 M 69,103 L 78,101','none','#af9883',1.3)
            s+=path('M 39,88 Q 34,88 35,93 Q 36,97 41,95 M 86,88 Q 91,88 90,93 Q 89,97 85,95','#efab65')
        else:
            s+=path('M 45,84 L 46,100 Q 49,105 53,100 L 53,87 M 71,87 L 72,100 Q 76,105 79,100 L 80,84','#efab65')
    return s+'</g></svg>\n'

if __name__=='__main__':
    for pose in ['idle','walk','sleep','play','food','water','wash','bathroom']:
        for frame in range(4):
            (OUT/f'ginger-{pose}-{frame}.svg').write_text(sprite(pose,frame))

    props = {}
    bowl = path('M 18,73 L 25,99 Q 63,114 103,99 L 110,73 Z','#7caaa4')
    bowl += ellipse(64,73,46,12,'#d7ebe1',INK,2.2)
    bowl += path('M 30,94 Q 42,99 53,99','none','#c1dfd4',3)
    food = bowl + ellipse(64,73,37,7,'#aa7152')
    for x,y in [(37,72),(46,68),(55,74),(65,69),(75,75),(83,69),(93,74),(66,76)]:
        food += ellipse(x,y,5,3,'#80523a')
    # Small fish emblem on the bowl identifies food without text.
    food += path('M 63,96 Q 71,87 81,95 Q 72,104 63,96 L 58,101 L 58,91 Z','#fff0d4','none')
    food += ellipse(77,95,1,1,INK)
    props['food'] = food
    water = bowl + ellipse(64,73,37,7,'#85c6d6')
    water += path('M 38,73 Q 51,68 62,73 M 70,75 Q 81,71 91,74','none','#e4fcff',2)
    water += path('M 66,86 Q 54,101 65,103 Q 78,102 66,86 Z','#e4fcff','none')
    props['water'] = water
    tree = ellipse(65,116,38,5,'#3b2430').replace('fill="#3b2430"','fill="#3b2430" opacity=".13"')
    tree += path('M 57,111 L 59,64 L 73,64 L 75,111 L 83,116 L 49,116 Z','#ad7959')
    tree += path('M 66,109 L 66,78 M 65,92 L 57,84','none','#78503f',2)
    tree += path('M 30,78 Q 14,72 23,57 Q 15,43 31,35 Q 27,20 45,19 Q 56,4 70,16 Q 89,9 96,28 Q 115,31 106,49 Q 119,63 102,75 Q 90,88 76,79 Q 61,92 49,80 Q 39,86 30,78 Z','#86ad85')
    tree += path('M 30,49 Q 30,38 42,37 Q 38,27 50,26 M 56,19 Q 63,16 69,23 M 84,29 Q 95,31 94,41','none','#bbcf9a',4)
    tree += path('M 34,67 Q 43,75 53,67 M 70,71 Q 83,78 91,66','none','#638e70',3)
    tree += path('M 86,99 Q 98,87 105,97 Q 99,107 86,99 M 88,99 L 101,97','#86ad85',INK,1.7)
    props['tree'] = tree
    bed = ellipse(64,110,51,6,'#3b2430').replace('fill="#3b2430"','fill="#3b2430" opacity=".13"')
    bed += path('M 11,91 Q 8,63 27,57 Q 62,43 101,58 Q 120,65 117,94 Q 112,114 65,114 Q 19,114 11,91 Z','#b894a0')
    bed += path('M 20,87 Q 17,67 38,63 Q 67,55 96,65 Q 111,69 108,88 Q 102,102 64,103 Q 29,102 20,87 Z','#f7dbbc')
    bed += path('M 23,87 Q 61,71 105,87 Q 101,103 64,104 Q 33,103 23,87 Z','#e7bf9c','none')
    bed += path('M 15,91 Q 23,102 43,103 L 48,93 Q 63,87 79,94 L 84,103 Q 107,101 114,91 L 114,100 Q 104,116 63,116 Q 24,115 14,101 Z','#b894a0')
    bed += path('M 52,103 Q 64,99 75,103','none','#e2bdc1',3)
    props['bed'] = bed
    for name,body in props.items():
        (OUT/f'prop-{name}.svg').write_text('<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><title>CodePet '+name+' prop</title>'+body+'</svg>\n')
