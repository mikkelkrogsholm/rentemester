//! Independent, offline backup check; no Session, server startup or restore authority.
use crate::backup_verify_state::{self as state, Selection, Verified};
use gpui_kit::{
    component::{Disableable, button::Button, checkbox::Checkbox},
    *,
};
use std::path::PathBuf;

pub struct BackupVerify {
    server_binary: PathBuf,
    archive: Option<Selection>,
    key: Option<Selection>,
    trusted: bool,
    busy: bool,
    verified: Option<Verified>,
    status: String,
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
}
impl BackupVerify {
    pub fn new(server_binary: PathBuf, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            server_binary,
            archive: None,
            key: None,
            trusted: false,
            busy: false,
            verified: None,
            status: "Vælg backupfilen og en separat betroet offentlig nøgle. Ingen kilde eller login er nødvendig.".into(),
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
        }
    }
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }
    fn choose(&mut self, key: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if key {
                    "Vælg den særskilt betroede offentlige RAW-nøgle (32 bytes)"
                } else {
                    "Vælg den eksporterede native backup (højst 96 MiB)"
                }
                .into(),
            ),
        });
        self.busy = true;
        self.trusted = false;
        self.verified = None;
        cx.spawn(async move |view, cx| {
            let path = match choice.await {
                Ok(Ok(Some(paths))) => paths.first().cloned(),
                Ok(Ok(None)) => None,
                _ => {
                    let _ = view.update(cx, |this, cx| {
                        this.busy = false;
                        this.status = "Filvælgeren kunne ikke åbnes".into();
                        cx.notify();
                    });
                    return;
                }
            };
            let result = if let Some(path) = path {
                Some(cx.background_executor().spawn(async move { Selection::read(&path, key) }).await)
            } else { None };
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Some(Ok(selection)) => {
                        if key { this.key = Some(selection); } else { this.archive = Some(selection); }
                        this.status = "Filens præcise bytes er fastholdt. Kontrollér fingeraftryk og nøglens uafhængige oprindelse før kontrollen.".into();
                    }
                    Some(Err(error)) => this.status = error,
                    None => {}
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn verify(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.trusted {
            return;
        }
        let (Some(archive), Some(key)) = (self.archive.clone(), self.key.clone()) else {
            return;
        };
        let binary = self.server_binary.clone();
        self.busy = true;
        self.verified = None;
        self.trusted = false;
        self.status = "Kontrollerer de gennemgåede bytes lokalt: signatur, inventar, schema, journal og originaler…".into();
        let task = cx
            .background_executor()
            .spawn(async move { state::verify(&binary, &archive, &key) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(verified) => {
                        this.verified = Some(verified);
                        this.status = "Backupkontrollen er gennemført for de viste fingeraftryk. Intet er gendannet, og ingen kildefil er ændret.".into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
impl Render for BackupVerify {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let mut view = div().flex().flex_col().gap_3().p_4()
            .child(div().text_xl().child("Kontrollér backup uafhængigt"))
            .child("Kontrollen foregår lokalt uden login eller adgang til kilde-workspacet. Vælg en offentlig nøgle, du har kontrolleret særskilt, f.eks. en tidligere gemt og betroet profilnøgle. En nøgle fra selve backupfilen etablerer aldrig tillid.")
            .child(self.status.clone())
            .child(Button::new("verify-choose-archive").track_focus(&self.entry_focus).label("Vælg backupfil").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.choose(false, cx))))
            .child(Button::new("verify-choose-key").label("Vælg separat betroet offentlig nøgle").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.choose(true, cx))));
        if let Some(archive) = &self.archive {
            view = view
                .child(format!(
                    "Backup: {} · {} bytes",
                    archive.name,
                    archive.size()
                ))
                .child(format!("Backup SHA-256: {}", archive.sha256));
        }
        if let Some(key) = &self.key {
            view = view
                .child(format!(
                    "Offentlig nøgle: {} · 32 rå Ed25519-bytes",
                    key.name
                ))
                .child(format!("Nøglens SHA-256-fingeraftryk: {}", key.sha256));
        }
        view = view.child(Checkbox::new("verify-trust-key")
            .label("Jeg har kontrolleret den offentlige nøgle og dens fingeraftryk via en uafhængig betroet kilde.")
            .checked(self.trusted).disabled(self.busy || self.archive.is_none() || self.key.is_none())
            .on_click(cx.listener(|this, checked, _, cx| { this.trusted = *checked; cx.notify(); })))
            .child(Button::new("verify-backup").label("Kontrollér de valgte bytes én gang")
                .disabled(self.busy || !self.trusted || self.archive.is_none() || self.key.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.verify(cx))));
        if let Some(verified) = &self.verified {
            view = view.child(div().text_lg().child("Backup verificeret · ingen gendannelse"))
                .child(format!("Format: {} · {} virksomheder", state::FORMAT, verified.companies))
                .child("Den betroede nøgle bekræfter det signerede inventar. Det native kontrolprogram har også valideret aktuelle understøttede schemaer, journalernes kæder og de originale kildefiler.")
                .child(format!("Kontrolgrundlag: {}", state::SCOPE));
        }
        view.child("Omfanget er backupfilens understøttede workspaceinventar med stabile virksomhedsidentiteter. Loginoplysninger, levende adgangsrettigheder, driftsnøgler og private signeringsnøgler er udeladt. Kontrollen lover hverken fuld legacy-dækning eller en gennemført gendannelse. Originale kildefiler og valgt nøgle ændres ikke; private midlertidige kontrolkopier fjernes efter kontrollen.")
    }
}
