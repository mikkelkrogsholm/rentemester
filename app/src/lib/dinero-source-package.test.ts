import {expect,test,vi} from "bun:test";
import {DINERO_SOURCE_LIMITS,fileSource,folderSource} from "./dinero-source-package";

function file(path:string,bytes:Uint8Array|string="synthetic"){const f=new File([typeof bytes==="string"?bytes:new Uint8Array(bytes).buffer],path.split("/").at(-1)!);Object.defineProperty(f,"webkitRelativePath",{value:path});return f;}
const unpack=(source:{contentBase64:string})=>JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(source.contentBase64),c=>c.charCodeAt(0))));
test("folder preserves exact binary, Unicode and unknown files and strips exactly one root deterministically",async()=>{
 const a=file("Export/2025/Posteringer.csv","\ufefforiginal\r\n"),b=file("Export/Ikke-bogførte-bilag/ukendt.bin",new Uint8Array([0,255,128,42]));
 const first=await folderSource([b,a]),again=await folderSource([a,b]);expect(first).toEqual(again);const p=unpack(first.source);expect(p.format).toBe("rentemester-dinero-source/1");expect(p.currency).toBe("DKK");expect(p.files.map((f:any)=>f.path)).toEqual(["2025/Posteringer.csv","Ikke-bogførte-bilag/ukendt.bin"]);expect([...Uint8Array.from(atob(p.files[1].contentBase64),c=>c.charCodeAt(0))]).toEqual([0,255,128,42]);expect(new TextDecoder().decode(first.bytes)).toContain("Ikke-bogførte-bilag");
 const nested=await folderSource([file("Export/wrapper/2025/Kontoplan.csv")]);expect(unpack(nested.source).files[0].path).toBe("wrapper/2025/Kontoplan.csv");
});
test("unsafe, colliding, mixed-root and over-bound folder metadata refuses before file reads",async()=>{
 for(const paths of [["Export/../escape"],["Export/C:/escape"],["Export/a\\b"],["Export/./x"],["Export//x"],["Export/a\u0000b"],["/Export/x"],["plain"],["Export/a","Other/b"],["Export/A","Export/a"],["Export/same","Export/same"]]){const files=paths.map(p=>file(p));const reads=files.map(f=>vi.spyOn(f,"arrayBuffer"));await expect(folderSource(files)).rejects.toThrow();for(const read of reads)expect(read).not.toHaveBeenCalled();}
 const large=file("Export/large");Object.defineProperty(large,"size",{value:DINERO_SOURCE_LIMITS.file+1});const read=vi.spyOn(large,"arrayBuffer");await expect(folderSource([large])).rejects.toThrow();expect(read).not.toHaveBeenCalled();
 const total=[file("Export/a"),file("Export/b")];for(const f of total)Object.defineProperty(f,"size",{value:10*1024*1024});await expect(folderSource(total)).rejects.toThrow();await expect(folderSource([])).rejects.toThrow();await expect(folderSource(Array.from({length:4097},(_,i)=>file("Export/"+i)))).rejects.toThrow();
});
test("changed read size refuses; ZIP and JSON stay byte-identical without local interpretation",async()=>{
 const changed=file("Export/changed");vi.spyOn(changed,"arrayBuffer").mockResolvedValue(new ArrayBuffer(0));await expect(folderSource([changed])).rejects.toThrow("ændrede størrelse");
 const raw=new Uint8Array([0x50,0x4b,3,4,255,0,42]);const f=new File([raw],"Raw.ZIP",{type:"application/zip"});const result=await fileSource(f);expect(result.bytes).toEqual(raw);expect(result.source.fileName).toBe("Raw.ZIP");expect([...Uint8Array.from(atob(result.source.contentBase64),c=>c.charCodeAt(0))]).toEqual([...raw]);await expect(fileSource(new File(["x"],"wrong.txt"))).rejects.toThrow();
});
