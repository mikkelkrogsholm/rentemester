/** Browser-only source transport. Accounting parsing and source interpretation stay on the server. */
import type { DineroSource } from "./api/dinero-import";

const MiB=1024*1024;
export const DINERO_SOURCE_LIMITS={container:25*MiB,file:16*MiB,total:18*MiB,files:4096};
export function sourceBase64(bytes:Uint8Array){let raw="";for(let i=0;i<bytes.length;i+=8192)raw+=String.fromCharCode(...bytes.subarray(i,i+8192));return btoa(raw);}
function safePath(path:string){
 if(!path||new TextEncoder().encode(path).length>1024||path.startsWith("/")||/^[a-z]:/i.test(path)||/[\\\u0000-\u001f\u007f-\u009f]/.test(path)||path.split("/").some(p=>!p||p==="."||p===".."))throw Error("Eksportmappen indeholder en ugyldig filsti.");
}
function exactOrder(a:string,b:string){const enc=new TextEncoder(),x=enc.encode(a),y=enc.encode(b);for(let i=0;i<Math.min(x.length,y.length);i++)if(x[i]!==y[i])return x[i]!-y[i]!;return x.length-y.length;}
/** Strip only the common directory actually selected by the browser; never infer an export layout. */
export async function folderSource(files:readonly File[]):Promise<{source:DineroSource;bytes:Uint8Array}>{
 if(!files.length||files.length>DINERO_SOURCE_LIMITS.files)throw Error("Vælg en eksportmappe med 1–4096 filer.");
 let root:string|undefined,total=0;const paths=new Set<string>();
 const entries=files.map(file=>{const relative=file.webkitRelativePath;safePath(relative);const slash=relative.indexOf("/");if(slash<1)throw Error("Vælg selve eksportmappen i mappevælgeren.");const selectedRoot=relative.slice(0,slash);if(root!==undefined&&root!==selectedRoot)throw Error("Vælg én samlet eksportmappe.");root=selectedRoot;const path=relative.slice(slash+1);safePath(path);const folded=path.toLowerCase();if(paths.has(folded))throw Error("Eksportmappen indeholder filstier, der kolliderer.");paths.add(folded);total+=file.size;if(file.size>DINERO_SOURCE_LIMITS.file||total>DINERO_SOURCE_LIMITS.total)throw Error("Eksportmappen må højst fylde 18 MiB, og hver fil højst 16 MiB.");return{path,file};}).sort((a,b)=>exactOrder(a.path,b.path));
 const packed=[];for(const entry of entries){const bytes=new Uint8Array(await entry.file.arrayBuffer());if(bytes.length!==entry.file.size)throw Error("En kildefil ændrede størrelse under læsning. Vælg mappen igen.");packed.push({path:entry.path,contentBase64:sourceBase64(bytes)});}
 const bytes=new TextEncoder().encode(JSON.stringify({format:"rentemester-dinero-source/1",currency:"DKK",files:packed}));
 if(bytes.length>DINERO_SOURCE_LIMITS.container)throw Error("Den samlede kildepakke må højst fylde 25 MiB.");
 return{bytes,source:{fileName:"dinero-folder.json",contentBase64:sourceBase64(bytes)}};
}
/** Keep the selected raw ZIP/JSON snapshot intact, including unknown ZIP inventory. */
export async function fileSource(file:File):Promise<{source:DineroSource;bytes:Uint8Array}>{
 if(!/\.(json|zip)$/i.test(file.name)||!file.size||file.size>DINERO_SOURCE_LIMITS.container)throw Error("Vælg en JSON-kildepakke eller ZIP-fil på højst 25 MiB.");
 const bytes=new Uint8Array(await file.arrayBuffer());if(bytes.length!==file.size)throw Error("Kildefilen ændrede størrelse under læsning. Vælg filen igen.");
 return{bytes,source:{fileName:file.name,contentBase64:sourceBase64(bytes)}};
}
