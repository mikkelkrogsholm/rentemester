//! Explicit ephemeral active-app dispatcher. Root retains this entity and invalidates it on session/connection/local lifecycle changes.
use crate::{
    Session, readable, scheduled_dispatcher_state as state, scheduled_recurring,
    scheduled_recurring_state as queue,
};
use gpui_kit::{
    component::{Disableable, button::Button},
    *,
};
use serde_json::Value;
use state::{Binding, Decision, Gate, Lane, LanePermit};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
struct Job {
    session: Arc<Session>,
    binding: Binding,
    gate: Arc<Gate>,
    next: Instant,
    message: String,
    proof: Option<Value>,
}
pub struct ScheduledDispatcher {
    jobs: Vec<Job>,
    lane: Lane,
    timer_armed: bool,
    message: String,
}
impl ScheduledDispatcher {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self{jobs:vec![],lane:Lane::new(),timer_armed:false,message:"Automatik er ikke aktiveret i denne app-session. Åbning/genstart/genforbindelse starter intet.".into()}
    }
    pub fn is_busy(&self) -> bool {
        self.lane.busy()
    }
    pub(crate) fn reserve_manual(&self) -> Result<LanePermit, String> {
        self.lane.reserve().ok_or("En gennemgået planlagt handling kører allerede. Vent på kanonisk kvittering eller afklar uafklaret udfald.".into())
    }
    pub fn invalidate(&mut self, cx: &mut Context<Self>) {
        for j in &self.jobs {
            j.gate.stop();
        }
        self.jobs.clear();
        self.message="Aktiv-app-autoritet er ugyldiggjort ved ændret session/forbindelse/lokal backend. Ingen ny dispatch. En allerede optaget request kan færdiggøres; dens markør/kvittering afgør udfaldet.".into();
        cx.notify();
    }
    pub fn stop_all(&mut self, cx: &mut Context<Self>) {
        for j in &mut self.jobs {
            j.gate.stop();
            j.message="Stoppet eksplicit i denne app. Allerede optaget request kan færdiggøres; intet gentages.".into();
        }
        self.message="Automatik stoppet i denne app. Serverkø/samtykke er ikke ændret; genstart kræver ny eksplicit bekræftet start.".into();
        cx.notify();
    }
    pub(crate) fn stop_template(&mut self, company: &str, id: i64, cx: &mut Context<Self>) {
        for j in &mut self.jobs {
            if j.binding.company == company && j.binding.template_id == id {
                j.gate.stop();
                j.message="Stoppet ved eksplicit manuel køhandling; genstart kræver ny bekræftet aktivering.".into();
            }
        }
        cx.notify();
    }
    /// Only called after the human's confirmed enable/resume or separately frozen local-start review.
    pub(crate) fn activate(
        &mut self,
        session: Arc<Session>,
        origin: &str,
        local: Option<&str>,
        data: &queue::Snapshot,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let binding = Binding::reviewed(origin, local, data)?;
        if queue::load()?.is_some() {
            return Err("Aktiv-app-start afvist: et forsøg kræver manuel afklaring; statuslæsning frigiver aldrig markøren".into());
        }
        // One authenticated connection lifetime. Company/screen switches do not alter this pin.
        if self.jobs.iter().any(|j| {
            !Arc::ptr_eq(&j.session, &session)
                || j.binding.origin != binding.origin
                || j.binding.local != binding.local
        }) {
            self.invalidate(cx);
        }
        for j in &self.jobs {
            if j.binding.company == binding.company && j.binding.template_id == binding.template_id
            {
                j.gate.stop();
            }
        }
        self.jobs.retain(|j| {
            !(j.binding.company == binding.company && j.binding.template_id == binding.template_id)
        });
        self.jobs.push(Job{session,binding,gate:Arc::new(Gate::new()),next:Instant::now(),message:"Eksplicit aktiveret i denne app-session; læser frisk serverplan før dagens eventuelle normale forekomst.".into(),proof:None});
        self.message="Automatik må kun køre, mens denne app-session og aktuel adgang er gyldig. Kun serverens normale forekomst for i dag; missed/kø/fejl kræver manuel kontrol. Ingen bogføring eller mail.".into();
        self.tick(cx);
        cx.notify();
        Ok(())
    }
    fn arm(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if self.timer_armed {
            return;
        }
        self.timer_armed = true;
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |view, cx| {
            timer.await;
            let _ = view.update(cx, |this, cx| {
                this.timer_armed = false;
                this.tick(cx);
            });
        })
        .detach();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if !self.jobs.iter().any(|j| j.gate.active()) {
            return;
        }
        if self.lane.busy() {
            self.arm(Duration::from_secs(1), cx);
            return;
        }
        let now = Instant::now();
        let Some(index) = self
            .jobs
            .iter()
            .position(|j| j.gate.active() && j.next <= now)
        else {
            let delay = self
                .jobs
                .iter()
                .filter(|j| j.gate.active())
                .map(|j| j.next.saturating_duration_since(now))
                .min()
                .unwrap_or(Duration::from_secs(60));
            self.arm(delay, cx);
            return;
        };
        let Some(permit) = self.lane.reserve() else {
            self.arm(Duration::from_secs(1), cx);
            return;
        };
        let j = &mut self.jobs[index];
        j.message="Kontrollerer frisk aktuel bruger/adgang/ledger, immutable skabelon, stateHash og readonly plan; ingen statuslæsning udsteder noget.".into();
        let (session, binding, gate) = (j.session.clone(), j.binding.clone(), j.gate.clone());
        let callback_gate = gate.clone();
        let task=cx.background_executor().spawn(async move{
   let _permit=permit;
   if !gate.active(){return Err("Stoppet før frisk læsning; ingen dispatch".to_string());}
   let data=scheduled_recurring::read(&session,&binding.company,Some(binding.template_id))?;
   let pending=queue::load()?.is_some();let decision=state::next(&binding,&data,pending)?;
   match decision{
    Decision::Idle=>Ok(None),
    Decision::Dispatch(review)=>{
     let marker=review.marker(&binding.origin,binding.local.as_deref());
     gate.admit(||queue::begin(&marker))?;
     // Admission precedes the one request. Stop/invalidate never turns it into a retry.
     if !gate.active(){gate.finish();return Err("Aktiv-app-kontekst blev ugyldiggjort efter markering; ingen POST. Markøren bevares til manuel kontrol.".into());}
     let result=session.runtime.block_on(session.client.mutate_scheduled_recurring(&binding.company,review.action.sdk(),review.body.as_ref().clone(),true));
     let v=match result{Ok(v)=>v,Err(e)=>{gate.finish();return Err(format!("Dispatch fejlede/er uafklaret: {}. Markøren BEVARES; kontrollér kø/fakturaer manuelt. Intet gentages.",readable(e)));}};
     let document_id=match state::receipt(&review,&v){Ok(Some(id))=>id,Ok(None)=>{gate.finish();return Err("Kendt terminal failed-dispatch modtaget. Automatik stoppet og markøren bevaret til manuel review/afklaring; ingen automatisk retry.".into());},Err(e)=>{gate.finish();return Err(e);}};
     let current=scheduled_recurring::read(&session,&binding.company,Some(binding.template_id))?;
     if current.principal!=binding.principal||current.ledger!=binding.ledger||current.company!=binding.company{return Err("Aktuel readback-identitet ændrede sig; markøren bevares til manuel afklaring".into());}
     let pdf=session.runtime.block_on(session.client.invoice_pdf(&binding.company,document_id)).map_err(readable)?;
     let proof=state::verified_original(&review,&v,&current,&pdf)?;
     queue::finish(&marker,false)?;gate.finish();Ok(Some(proof))
    }
   }
  });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{
   if let Some(j)=this.jobs.iter_mut().find(|j|Arc::ptr_eq(&j.gate,&callback_gate)){
    match result{Ok(proof)=>{if let Some(proof)=proof{j.proof=Some(proof);j.message="Præcis template/period/documentId, kanonisk udstedt-ubogført-ikke-afsendt faktura og original PDF er genlæst. Ingen bogføring/mail.".into();}else{j.message="Ingen normal forekomst for serverens i dag. Kun readonly kontrol udført; næste kontrol om cirka ét minut, mens app/session er aktiv.".into();}j.next=Instant::now()+Duration::from_secs(60);},Err(e)=>{j.gate.stop();j.message=e;}}
   }
   this.tick(cx);cx.notify();
  });}).detach();
        cx.notify();
    }
}
impl Drop for ScheduledDispatcher {
    fn drop(&mut self) {
        for j in &self.jobs {
            j.gate.stop();
        }
    }
}
impl Render for ScheduledDispatcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.jobs.iter().filter(|j| j.gate.active()).count();
        let mut view=div().flex().flex_col().gap_2().child(div().text_lg().child(format!("Aktiv-app fakturaautomatik · {active} eksplicit aktiverede skabeloner"))).child(self.message.clone())
   .child("Ingen unattended service. Lukning/søvn/logout/ændret forbindelse stopper eller udskyder appens arbejde; genåbning aktiverer ikke automatisk. Serverens faktisk missed/kø/fejl kræver manuel review. En allerede optaget request kan færdiggøres; uafklaret markør bevares.")
   .child(Button::new("scheduled-dispatcher-stop-all").label("Stop automatik i denne app").disabled(active==0).on_click(cx.listener(|this,_,_,cx|this.stop_all(cx))));
        if self.lane.busy() {
            view=view.child("Én planlagt handling er optaget i den fælles manuelle/automatiske lane. Ingen anden dispatch optages samtidig.");
        }
        for j in &self.jobs {
            let (company, id) = (j.binding.company.clone(), j.binding.template_id);
            view = view.child(format!(
                "{} · skabelon {} · bruger {} · {}",
                company, id, j.binding.principal, j.message
            ));
            if let Some(proof) = &j.proof {
                view = view.child(format!("Kanonisk kvittering/PDF-kontrol: {proof}"));
            }
            view = view.child(
                Button::new(SharedString::from(format!(
                    "scheduled-dispatcher-stop-{company}-{id}"
                )))
                .label(format!("Stop {} / {} i denne app", company, id))
                .disabled(!j.gate.active())
                .on_click(cx.listener(move |this, _, _, cx| this.stop_template(&company, id, cx))),
            );
        }
        view
    }
}
