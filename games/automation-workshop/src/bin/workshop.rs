use automation_workshop::{Workshop,Definitions,Command,Envelope};
use vesper3d::runtime::{Simulation,snapshot};
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(1)}}
fn run()->Result<(),String>{
    let args:Vec<_>=std::env::args().skip(1).collect();let action=args.first().map(String::as_str).unwrap_or("inspect");let mut game=Workshop::standard();
    let out=match action{
        "validate"=>{if let Some(path)=args.get(1){game=Workshop::new(Definitions::load(std::path::Path::new(path)).map_err(|e|e.to_string())?,2)?;}serde_json::json!({"ok":true,"content":game.definitions.fingerprint(),"version":game.definitions.content_version})},
        "inspect"=>game.inspect(args.get(1).map(String::as_str).unwrap_or("workshop:machine-a"))?,
        "play"|"save-v1"=>{for(serial,machine)in [(1,"workshop:machine-a"),(2,"workshop:machine-b"),(3,"workshop:machine-a")]{game.step(&vec![Envelope{seat:0,serial,command:Command::Queue{machine:machine.into(),recipe:"workshop:smelt".into()}}]);}if action=="save-v1"{let bytes=snapshot::save(&game,"V1 in-flight workshop").map_err(|e|e.to_string())?;let path=args.get(1).ok_or("save-v1 needs output path (fixture generation only)")?;std::fs::write(path,&bytes).map_err(|e|e.to_string())?;serde_json::json!({"fixture":path,"bytes":bytes.len(),"tick":game.state.tick,"hash":game.state_hash()})}else{game.inspect("workshop:machine-a")?}},
        _=>return Err("Commands: validate [CONTENT], inspect [MACHINE-ID], play, save-v1 OUTPUT (fixture generation)".into()),
    };println!("{}",serde_json::to_string(&out).map_err(|e|e.to_string())?);Ok(())
}
