//! Credential-free diagnostic encoder: exactly the production Google translator.
use serde_json::json;
fn main() {
    let model=std::env::args().nth(1).expect("model");
    let effort=std::env::args().nth(2).unwrap_or_else(||"low".into());
    let mut body=json!({"model":model,"input":[{"role":"user","content":"Reply only OK."}],
        "reasoning":{"effort":effort,"summary":"none"},"max_output_tokens":32000,"stream":false});
    if std::env::args().any(|a|a=="--tools") {
        body["input"]=json!("Use functions.echo once with text OK, do not answer directly.");
        body["tools"]=json!([{"type":"namespace","name":"functions","tools":[{"type":"function","name":"echo","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}]);
    }
    match switcher_pet_compat_tests::antigravity::translate::responses_to_antigravity(&serde_json::to_vec(&body).unwrap(),&model,"synthetic-project") {
        Ok((bytes,_))=>println!("{}",String::from_utf8(bytes).unwrap()),
        Err(e)=>{eprintln!("{e}");std::process::exit(1);}
    }
}
