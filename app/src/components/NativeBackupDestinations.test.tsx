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
