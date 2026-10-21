import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeBackupDestinations } from "./NativeBackupDestinations";

test("lost placement reply persists exact operation and reload performs only explicit readback", async()=>{
 localStorage.clear();let posts=0,operation="";const reads:string[]=[];
 const mock=vi.fn(async(input:string|URL,init?:RequestInit)=>{
  const path=String(input);reads.push(path);
  if(path==="/api/workspace/backup-destinations")return Response.json({ok:true,workspaceId:"synthetic-workspace",destinations:[{id:"synthetic-destination",label:"Synthetic folder",kind:"local-folder",offsiteAttested:false}]});
  if(init?.method==="POST"){posts++;operation=JSON.parse(String(init.body)).operationId;throw Error("Synthetic reply lost after publication");}
  expect(path).toBe(`/api/workspace/backup-destinations/synthetic-destination/placements/${operation}`);
  return Response.json({ok:true,placement:{archiveSha256:"a".repeat(64),archiveSizeBytes:123,verified:true,auditRecorded:true,remoteSyncVerified:false}});
 });stubGlobal("fetch",mock);
 const first=renderAt(<NativeBackupDestinations/>);
 await userEvent.click(screen.getByRole("button",{name:"Vis backupdestinationer"}));
 await screen.findByLabelText("Backupdestination");
 await userEvent.click(screen.getByRole("checkbox"));await userEvent.click(screen.getByRole("button",{name:"Dan og placér backup"}));
 await screen.findByRole("alert");expect(posts).toBe(1);expect(operation).not.toBe("");
 expect(JSON.parse(localStorage.getItem("rentemester-backup-placement:synthetic-workspace")!).operationId).toBe(operation);
 first.unmount();renderAt(<NativeBackupDestinations/>);
 expect(posts).toBe(1);await userEvent.click(screen.getByRole("button",{name:"Vis backupdestinationer"}));
 await screen.findByText(`Placering: ${operation}`);expect(posts).toBe(1);
 await userEvent.click(screen.getByRole("button",{name:"Kontrollér placeringen"}));
 await screen.findByText(/Backupfilen er genlæst/);expect(posts).toBe(1);
 expect(reads.at(-1)).toContain(operation);
 expect(screen.getByText(/Gendannelse og synkronisering til skyen er ikke verificeret/)).toBeTruthy();
 await userEvent.click(screen.getByRole("button",{name:"Klargør næste backup"}));
 expect(localStorage.getItem("rentemester-backup-placement:synthetic-workspace")).toBeNull();
});

test("provider removal keeps a remembered remote operation reachable from the destinations panel", async () => {
 localStorage.clear();
 const intent={operationId:"synthetic-remote-114",placementOperationId:"synthetic-placement-114",remoteObjectId:"synthetic_drive_object"};
 localStorage.setItem("rentemester-remote-backup:synthetic-workspace:synthetic-drive",JSON.stringify(intent));
 const mock=vi.fn(async(input:string|URL,init?:RequestInit)=>{
  expect(init?.method).not.toBe("POST");
  if(String(input)==="/api/workspace/backup-destinations")return Response.json({ok:true,workspaceId:"synthetic-workspace",destinations:[{id:"synthetic-drive",label:"Synthetic Drive",kind:"google-drive",offsiteAttested:false}],remoteVerificationDestinations:[]});
  expect(String(input)).toContain(`/remote-verifications/${intent.operationId}`);
  return Response.json({ok:true,verification:{...intent,destinationId:"synthetic-drive",archiveSha256:"a".repeat(64),archiveSizeBytes:123,observedAt:"2026-10-21T00:00:00Z",remoteSyncVerified:true}});
 });stubGlobal("fetch",mock);renderAt(<NativeBackupDestinations/>);
 await userEvent.click(screen.getByRole("button",{name:"Vis backupdestinationer"}));
 await screen.findByText(/Ny Drive-kontrol er deaktiveret/);
 await userEvent.click(screen.getByRole("button",{name:"Hent fjernkopiens kvittering"}));
 await screen.findByText(/Serveren verificerede fjernkopiens bytes/);
 expect(mock).toHaveBeenCalledTimes(2);
});
