//! Thin read adapters; static coverage grants no company authority.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;
pub(crate) fn validate_date(s: &str) -> Result<(), Error> {
    let b=s.as_bytes();
    if b.len()!=10 || b[4]!=b'-' || b[7]!=b'-' || b.iter().enumerate().any(|(i,c)|i!=4&&i!=7&&!c.is_ascii_digit()) { return Err(Error::InvalidInput("date must be YYYY-MM-DD")); }
    let y=s[..4].parse::<u32>().unwrap(); let m=s[5..7].parse::<u32>().unwrap(); let d=s[8..].parse::<u32>().unwrap();
    let max=match m { 4|6|9|11=>30,2=>if y%4==0&&(y%100!=0||y%400==0){29}else{28},1|3|5|7|8|10|12=>31,_=>0 };
    if y==0||d==0||d>max { return Err(Error::InvalidInput("invalid calendar date")); } Ok(())
}
impl ApiClient {
    pub async fn regulatory_coverage(&self)->Result<Value,Error>{self.get("/api/regulatory/coverage",&[]).await}
    pub async fn regulatory_citations(&self)->Result<Value,Error>{self.get("/api/regulatory/citations",&[]).await}
    pub async fn compliance_report(&self,company:&str,as_of:Option<&str>)->Result<Value,Error>{
        let mut query=vec![]; if let Some(date)=as_of {validate_date(date)?;query.push(("asOf",date));}
        self.get(&company_path(company,"compliance-report")?,&query).await
    }
}
