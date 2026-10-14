"""Referencia escalar para validar especificaciones de Ommatix.

No es el motor productivo: no hay R*-tree, SIMD, OCR, detector, tracking
perceptivo, MCP ni actuador. Las identidades usadas en Registry se suministran
explícitamente. Todos los puntos devueltos son candidatos de simulación.
"""
from __future__ import annotations
from dataclasses import dataclass, replace
import math
import unicodedata
from typing import Iterable

@dataclass(frozen=True)
class Rect:
    x0: float
    y0: float
    x1: float
    y1: float

    def __post_init__(self) -> None:
        if not all(math.isfinite(v) for v in (self.x0,self.y0,self.x1,self.y1)):
            raise ValueError('NON_FINITE_RECT')
        if self.x1 <= self.x0 or self.y1 <= self.y0:
            raise ValueError('EMPTY_RECT')

    @property
    def area(self) -> float:
        return (self.x1-self.x0)*(self.y1-self.y0)

    @property
    def height(self) -> float:
        return self.y1-self.y0

    @property
    def center(self) -> tuple[float,float]:
        return ((self.x0+self.x1)/2,(self.y0+self.y1)/2)

    def intersection(self, other: Rect) -> float:
        return max(0.0,min(self.x1,other.x1)-max(self.x0,other.x0))*max(0.0,min(self.y1,other.y1)-max(self.y0,other.y0))

    def ioc(self, child: Rect) -> float:
        return self.intersection(child)/child.area

    def union(self,other: Rect) -> Rect:
        return Rect(min(self.x0,other.x0),min(self.y0,other.y0),max(self.x1,other.x1),max(self.y1,other.y1))

    def contains(self, other: Rect) -> bool:
        return self.x0 <= other.x0 and self.y0 <= other.y0 and self.x1 >= other.x1 and self.y1 >= other.y1

@dataclass(frozen=True)
class Frame:
    width: int
    height: int
    stride: int
    pixels: bytes
    epoch: int = 1

    def __post_init__(self) -> None:
        if not all(isinstance(v,int) and not isinstance(v,bool) for v in (self.width,self.height,self.stride)):
            raise ValueError('INVALID_DIMENSIONS')
        if self.width <= 0 or self.height <= 0 or self.width*self.height > 20_000_000:
            raise ValueError('INVALID_DIMENSIONS')
        if self.stride < 4*self.width or len(self.pixels) != self.stride*self.height:
            raise ValueError('INVALID_BUFFER')


def dirty_tiles(previous: Frame | None,current: Frame,tile_size: int=64) -> set[tuple[int,int]]:
    """Comparación BGRA exacta, sin alfa ni padding; no altera ningún estado."""
    if not isinstance(tile_size,int) or isinstance(tile_size,bool) or tile_size <= 0:
        raise ValueError('INVALID_TILE_SIZE')
    cols=(current.width+tile_size-1)//tile_size
    rows=(current.height+tile_size-1)//tile_size
    if previous is None or (previous.width,previous.height,previous.epoch)!=(current.width,current.height,current.epoch):
        return {(x,y) for y in range(rows) for x in range(cols)}
    dirty=set()
    for ty in range(rows):
        for tx in range(cols):
            changed=False
            for y in range(ty*tile_size,min((ty+1)*tile_size,current.height)):
                for x in range(tx*tile_size,min((tx+1)*tile_size,current.width)):
                    old=y*previous.stride+4*x
                    new=y*current.stride+4*x
                    if previous.pixels[old:old+3] != current.pixels[new:new+3]:
                        dirty.add((tx,ty));changed=True;break
                if changed: break
    return dirty


def halo_close(region: Rect,supports: Iterable[Rect],bounds: Rect,max_rounds: int=1000) -> Rect:
    """Cierre transitivo sobre soportes conocidos, sin índice espacial."""
    if not bounds.contains(region): raise ValueError('REGION_OUT_OF_BOUNDS')
    supports=tuple(supports)
    if any(not bounds.contains(s) for s in supports): raise ValueError('SUPPORT_OUT_OF_BOUNDS')
    for _ in range(max_rounds):
        old=region
        for support in supports:
            if region.intersection(support)>0: region=region.union(support)
        if old==region:return region
    return bounds  # límite conservador: refresco completo


def merge_regions(regions: Iterable[Rect]) -> list[Rect]:
    work=sorted(regions,key=lambda r:(r.x0,r.y0,r.x1,r.y1))
    changed=True
    while changed:
        changed=False
        for i in range(len(work)):
            for j in range(i+1,len(work)):
                if work[i].intersection(work[j])>0:
                    work[i]=work[i].union(work.pop(j));changed=True;break
            if changed:break
    return sorted(work,key=lambda r:(r.x0,r.y0,r.x1,r.y1))

@dataclass(frozen=True)
class Word:
    text: str
    bbox: Rect


def normalized(text: str) -> str:
    return ' '.join(unicodedata.normalize('NFC',text).casefold().split())


def reading_order(words: Iterable[Word]) -> list[Word]:
    """Bandas con ancla fija; no usa comparador no transitivo por pareja."""
    ordered=sorted(words,key=lambda w:(w.bbox.center[1],w.bbox.x0,w.text))
    lines: list[list[Word]]=[]
    for word in ordered:
        possible=[]
        for i,line in enumerate(lines):
            anchor=line[0]
            delta=abs(word.bbox.center[1]-anchor.bbox.center[1])
            if delta <= 0.5*min(word.bbox.height,anchor.bbox.height): possible.append((delta,i))
        if possible:lines[min(possible)[1]].append(word)
        else:lines.append([word])
    return [word for line in lines for word in sorted(line,key=lambda w:(w.bbox.x0,w.bbox.y0,w.text))]


def internal_owners(controls: list[Rect],words: list[Word],threshold: float=.75) -> dict[int,list[Word]]:
    """Prefiere el contenedor más específico; empate de área => abstención."""
    if not 0 <= threshold <= 1:raise ValueError('INVALID_THRESHOLD')
    result={i:[] for i in range(len(controls))}
    for word in words:
        candidates=sorted((b.area,i) for i,b in enumerate(controls) if b.ioc(word.bbox)>=threshold)
        if not candidates:continue
        if len(candidates)>1 and math.isclose(candidates[0][0],candidates[1][0]):continue
        result[candidates[0][1]].append(word)
    return {i:reading_order(v) for i,v in result.items()}


def corridor(text: Rect,control: Rect) -> Rect | None:
    """Sólo espacio entre bordes; auxiliar limitado a la geometría LTR."""
    if text.x1 <= control.x0:
        y0=max(text.y0,control.y0);y1=min(text.y1,control.y1)
        if text.x1 < control.x0 and y0 < y1:return Rect(text.x1,y0,control.x0,y1)
    if text.y1 <= control.y0:
        x0=max(text.x0,control.x0);x1=min(text.x1,control.x1)
        if text.y1 < control.y0 and x0 < x1:return Rect(x0,text.y1,x1,control.y0)
    return None


def choose_external_label(control: Rect,words: list[Word],other_controls: list[Rect],margin: float=.15) -> str | None:
    """Selector local de referencia, no matching bipartito productivo."""
    candidates=[]
    for word in words:
        b=word.bbox;s=max(control.height,b.height,1)
        if b.x1 <= control.x0 and abs(b.center[1]-control.center[1])<=s*.8:
            cost=(control.x0-b.x1+2.5*abs(b.center[1]-control.center[1]))/s
        elif b.y1 <= control.y0 and abs(b.x0-control.x0)<=s:
            cost=(control.y0-b.y1+1.5*abs(b.x0-control.x0))/s
        else:continue
        if cost>6:continue
        bridge=corridor(b,control)
        blockers=[r for r in other_controls if r!=control and not r.contains(control)]
        if bridge and any(bridge.intersection(r)>0 for r in blockers):continue
        candidates.append((cost,word.text))
    candidates.sort()
    if not candidates or (len(candidates)>1 and candidates[1][0]-candidates[0][0]<margin):return None
    return candidates[0][1]


def legacy_dhash(gray: Iterable[int],tau: int=3) -> int:
    values=tuple(gray)
    if len(values)!=72:raise ValueError('EXPECTED_9X8')
    result=0
    for r in range(8):
        for c in range(8):
            if values[r*9+c]-values[r*9+c+1]>=tau:result|=1<<(r*8+c)
    return result


def legacy_distance(a:int,b:int) -> int:
    difference=((a^b)&((1<<64)-1)).bit_count()
    return min(difference,64-difference)

@dataclass(frozen=True)
class GradientDescriptor:
    bits:int
    mask:int


def gradient_descriptor(gray: Iterable[int],tau:int=3) -> GradientDescriptor:
    values=tuple(gray)
    if len(values)!=72 or any(not isinstance(x,int) or not 0<=x<=255 for x in values):raise ValueError('EXPECTED_9X8_U8')
    if not 1<=tau<=255:raise ValueError('INVALID_TAU')
    bits=mask=0
    for r in range(8):
        for c in range(8):
            difference=values[r*9+c]-values[r*9+c+1];shift=r*8+c
            if abs(difference)>=tau:
                mask|=1<<shift
                if difference>0:bits|=1<<shift
    return GradientDescriptor(bits,mask)


def descriptor_distance(a:GradientDescriptor,b:GradientDescriptor,min_support:int=8,min_overlap:float=.7) -> float | None:
    if min_support<1 or not 0<=min_overlap<=1:raise ValueError('INVALID_SUPPORT_POLICY')
    common=a.mask&b.mask;union=a.mask|b.mask;n=common.bit_count()
    if n<min_support or n/union.bit_count()<min_overlap:return None
    h=((a.bits^b.bits)&common).bit_count()
    return min(h,n-h)/n


def legacy_id(role:int,label:str,x:int,y:int) -> int:
    """Transcripción mínima de la fórmula del PDF para contraejemplos."""
    h=2166136261
    h=((h^role)*16777619)&0xffffffff
    for byte in label.lower().replace(' ','').encode():h=((h^byte)*16777619)&0xffffffff
    for byte in ((x>>6)&0xffff).to_bytes(2,'little')+((y>>6)&0xffff).to_bytes(2,'little'):
        h=((h^byte)*16777619)&0xffffffff
    return 10000+h%90000


def collision_probability(n:int,space:int=90000) -> float:
    if n<0 or space<=0:raise ValueError('INVALID_COUNTS')
    if n>space:return 1.0
    return 1.0-math.prod(1-k/space for k in range(n))

@dataclass(frozen=True)
class Pending:
    base_revision:int
    frame:Frame
    payload:tuple[str,...]

class SnapshotStore:
    """Modelo mínimo de commit; no existe percepción ni concurrencia real."""
    def __init__(self) -> None:
        self.revision=0;self.frame:Frame|None=None;self.payload:tuple[str,...]=()
    def stage(self,frame:Frame,payload:Iterable[str]) -> Pending:
        return Pending(self.revision,frame,tuple(payload))
    def commit(self,pending:Pending) -> None:
        if pending.base_revision!=self.revision:raise ValueError('STALE_COMMIT')
        self.frame=pending.frame;self.payload=pending.payload;self.revision+=1

@dataclass(frozen=True)
class Element:
    target_id:str
    revision:int
    bbox:Rect
    label:str
    actionability:str='unknown'
    enabled:bool=True
    visible:bool=True

class Registry:
    """IDs/revisiones con asociación suministrada por el test, no un tracker CV."""
    def __init__(self,session_id:str='demo-session') -> None:
        self.session_id=session_id;self.counter=0;self.snapshot=0;self.elements:dict[str,Element]={}
    def add(self,bbox:Rect,label:str,actionability:str='unknown') -> Element:
        self.counter+=1
        element=Element(f'e{self.counter}',1,bbox,label,actionability)
        self.elements[element.target_id]=element;self.snapshot+=1
        return element
    def update(self,target_id:str,**changes) -> Element:
        if any(k in changes for k in ('target_id','revision')):raise ValueError('IMMUTABLE_ID')
        old=self.elements[target_id];new=replace(old,revision=old.revision+1,**changes)
        self.elements[target_id]=new;self.snapshot+=1;return new
    def retire(self,target_id:str) -> None:
        del self.elements[target_id];self.snapshot+=1
    def prepare(self,session_id:str,snapshot:int,target_id:str,revision:int,*,dry_run:bool=True) -> dict:
        if session_id!=self.session_id:raise ValueError('SESSION_MISMATCH')
        if snapshot!=self.snapshot:raise ValueError('STALE_SNAPSHOT')
        if target_id not in self.elements:raise ValueError('TARGET_NOT_FOUND')
        item=self.elements[target_id]
        if revision!=item.revision:raise ValueError('STALE_TARGET')
        if item.actionability!='confirmed' or not item.enabled or not item.visible:raise ValueError('NOT_ACTIONABLE')
        if not dry_run:raise ValueError('ACTUATION_NOT_IMPLEMENTED')
        return {'status':'simulated','target_id':target_id,'input_delivered':False}
