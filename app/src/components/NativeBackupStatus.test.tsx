import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeBackupStatus } from "./NativeBackupStatus";
const status=(enforced=false,graceDays=0)=>({ok:true,slug:"synthetic",backup:{available:true,scope:"local-retained-snapshots",backupDue:false,hasActivitySinceBackup:true,latestBackupAt:"2026-10-21T00:00:00Z",requiredBy:"2026-10-28T00:00:00Z",checkedAt:"2026-10-22T00:00:00Z",backupsFound:1},lock:{enforced,graceDays,locked:false,lockDeadline:null}});
test("explicit load, current policy review and one confirmed exact save",async()=>{
 sessionStorage.clear();const fetchMock=vi.fn(async(_path:string,init?:RequestInit)=>Response.json(init?.method==="POST"?status(true,2):status()));stubGlobal("fetch",fetchMock);
 renderAt(<NativeBackupStatus slug="synthetic"/>);expect(fetchMock).toHaveBeenCalledTimes(0);
 await userEvent.click(screen.getByRole("button",{name:"Hent backupstatus"}));
 const save=screen.getByRole("button",{name:"Gem bogføringslås"});expect(save).toBeDisabled();
 await userEvent.click(screen.getByRole("checkbox",{name:/Lås bogføring, når/}));
 const grace=screen.getByRole("spinbutton",{name:"Ekstra frist i dage"});await userEvent.clear(grace);await userEvent.type(grace,"2");
 await userEvent.click(screen.getByRole("checkbox",{name:/Jeg har gennemgået/}));await userEvent.click(save);await screen.findByRole("status");
 expect(fetchMock).toHaveBeenCalledTimes(2);expect(fetchMock.mock.calls[1]![0]).toBe("/api/companies/synthetic/backup-lock");
 expect(JSON.parse(String(fetchMock.mock.calls[1]![1]?.body))).toEqual({confirm:true,enforced:true,graceDays:2});expect(save).toBeDisabled();
});
test("lost reply blocks writes across remount until an explicit successful status read",async()=>{
 sessionStorage.clear();const fetchMock=vi.fn(async(_path:string,init?:RequestInit)=>{if(init?.method==="POST")throw TypeError("lost reply");return Response.json(status(true,1));});stubGlobal("fetch",fetchMock);
 const view=renderAt(<NativeBackupStatus slug="synthetic"/>);await userEvent.click(screen.getByRole("button",{name:"Hent backupstatus"}));
 await userEvent.click(screen.getByRole("checkbox",{name:/Jeg har gennemgået/}));await userEvent.click(screen.getByRole("button",{name:"Gem bogføringslås"}));await screen.findByText(/Nye ændringer er blokeret/);
 expect(screen.getByRole("button",{name:"Gem bogføringslås"})).toBeDisabled();view.unmount();renderAt(<NativeBackupStatus slug="synthetic"/>);
 expect(screen.getByText(/Nye ændringer er blokeret/)).toBeInTheDocument();expect(fetchMock).toHaveBeenCalledTimes(2);
 await userEvent.click(screen.getByRole("button",{name:"Hent backupstatus"}));expect(screen.queryByText(/Nye ændringer er blokeret/)).toBeNull();
 expect(screen.getByRole("button",{name:"Gem bogføringslås"})).toBeDisabled();expect(screen.getByRole("checkbox",{name:/Lås bogføring, når/})).toBeChecked();
 expect(fetchMock.mock.calls.filter(([,init])=>init?.method==="POST")).toHaveLength(1);
});
test("failed reconciliation preserves the lock and invalid grace cannot be confirmed",async()=>{
 sessionStorage.clear();let failed=false;const fetchMock=vi.fn(async(_path:string,init?:RequestInit)=>{if(init?.method==="POST"){failed=true;return new Response("bad reply",{status:500});}if(failed)throw TypeError("read unavailable");return Response.json(status());});stubGlobal("fetch",fetchMock);
 renderAt(<NativeBackupStatus slug="synthetic"/>);await userEvent.click(screen.getByRole("button",{name:"Hent backupstatus"}));
 const grace=screen.getByRole("spinbutton",{name:"Ekstra frist i dage"});await userEvent.clear(grace);await userEvent.type(grace,"366");expect(screen.getByRole("checkbox",{name:/Jeg har gennemgået/})).toBeDisabled();
 await userEvent.clear(grace);await userEvent.type(grace,"0");await userEvent.click(screen.getByRole("checkbox",{name:/Jeg har gennemgået/}));await userEvent.click(screen.getByRole("button",{name:"Gem bogføringslås"}));await screen.findByText(/Nye ændringer er blokeret/);
 await userEvent.click(screen.getByRole("button",{name:"Hent backupstatus"}));expect(screen.getByText(/Nye ændringer er blokeret/)).toBeInTheDocument();expect(fetchMock.mock.calls.filter(([,init])=>init?.method==="POST")).toHaveLength(1);
});
