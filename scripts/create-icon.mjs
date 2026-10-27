import fs from 'node:fs'
const size=32,pixels=Buffer.alloc(size*size*4)
const segment=(px,py,x1,y1,x2,y2)=>{const t=Math.max(0,Math.min(1,((px-x1)*(x2-x1)+(py-y1)*(y2-y1))/((x2-x1)**2+(y2-y1)**2)));return Math.hypot(px-x1-t*(x2-x1),py-y1-t*(y2-y1))<1.2}
for(let y=0;y<size;y++)for(let x=0;x<size;x++){
 const i=((size-y-1)*size+x)*4
 const round=(x>=7&&x<25)||(y>=7&&y<25)||Math.hypot(x-(x<16?7:24),y-(y<16?7:24))<7
 const mark=[[16,9],[8,23],[24,23]].some(([cx,cy])=>Math.hypot(x-cx,y-cy)<3.5)||segment(x,y,16,9,8,23)||segment(x,y,16,9,24,23)||segment(x,y,8,23,24,23)
 pixels[i]=mark?255:232;pixels[i+1]=mark?255:121;pixels[i+2]=mark?255:51;pixels[i+3]=round?255:0
}
const dib=Buffer.alloc(40);dib.writeUInt32LE(40,0);dib.writeInt32LE(size,4);dib.writeInt32LE(size*2,8);dib.writeUInt16LE(1,12);dib.writeUInt16LE(32,14);dib.writeUInt32LE(pixels.length,20)
const bmp=Buffer.concat([dib,pixels,Buffer.alloc(size*4)])
const header=Buffer.alloc(22);header.writeUInt16LE(1,2);header.writeUInt16LE(1,4);header[6]=size;header[7]=size;header.writeUInt16LE(1,10);header.writeUInt16LE(32,12);header.writeUInt32LE(bmp.length,14);header.writeUInt32LE(22,18)
fs.mkdirSync(new URL('../src-tauri/icons/',import.meta.url),{recursive:true})
fs.writeFileSync(new URL('../src-tauri/icons/icon.ico',import.meta.url),Buffer.concat([header,bmp]))
