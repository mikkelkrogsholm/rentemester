//! Exclusive offline enrollment flags, never a request route.
pub fn validate(conflict:bool, configured:bool, confirmed:bool)->Result<(), &'static str>{
    if conflict || !configured || !confirmed {return Err("--enroll-namespace-custody requires exclusive --config PRIVATE --confirm-target EXACTROOT; server must be stopped");}
    Ok(())
}
#[cfg(test)]
mod tests{
    #[test]
    fn namespace_custody_exclusive_operator_flags(){
        assert!(super::validate(false,true,true).is_ok());
        for flags in [(true,true,true),(false,false,true),(false,true,false)]{assert!(super::validate(flags.0,flags.1,flags.2).is_err());}
    }
}
