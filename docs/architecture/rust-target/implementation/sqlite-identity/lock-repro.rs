use rentemester_runtime::{WorkspaceLease,WorkspaceLayout};
use rusqlite::Connection;
use std::{path::PathBuf,process::Command};
fn main(){let args:Vec<_>=std::env::args().collect();if args.get(1).map(String::as_str)==Some("child"){let c=Connection::open(&args[2]).unwrap();c.busy_timeout(std::time::Duration::from_millis(20)).unwrap();match c.execute_batch("BEGIN IMMEDIATE; ROLLBACK"){Ok(_)=>println!("acquired"),Err(_)=>println!("busy")};return;}
let root=PathBuf::from(&args[1]);std::fs::create_dir_all(root.join(".rentemester")).unwrap();std::fs::create_dir_all(root.join("a/data")).unwrap();std::fs::write(root.join("workspace.json"),r#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#).unwrap();
let control=Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();let company=Connection::open(root.join("a/data/ledger.sqlite")).unwrap();for c in [&control,&company]{c.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE synthetic(v)").unwrap();}
let mut layout=WorkspaceLayout::bind(WorkspaceLease::acquire(&root).unwrap(),"w","a").unwrap();control.execute_batch("BEGIN IMMEDIATE").unwrap();
let contender=||{String::from_utf8(Command::new(&args[0]).arg("child").arg(root.join(".rentemester/workspace-control.sqlite")).output().unwrap().stdout).unwrap()};
println!("before={}",contender().trim());layout.ensure_connections(&control,&company).unwrap();println!("after={}",contender().trim());println!("sqlite_still_in_transaction={}",!control.is_autocommit());control.execute_batch("ROLLBACK").unwrap();drop(control);drop(company);drop(layout);
}
