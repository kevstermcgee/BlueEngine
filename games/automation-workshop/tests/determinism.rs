use automation_workshop::{Workshop,Envelope,Command};
use vesper3d::runtime::{Simulation,assert_deterministic,snapshot};
fn inputs()->Vec<Vec<Envelope>>{(0..90).map(|tick|match tick{
 0=>vec![Envelope{seat:0,serial:1,command:Command::Queue{machine:"workshop:machine-a".into(),recipe:"workshop:smelt".into()}}],
 1=>vec![Envelope{seat:1,serial:1,command:Command::Queue{machine:"workshop:machine-b".into(),recipe:"workshop:smelt".into()}}],
 2=>vec![Envelope{seat:0,serial:2,command:Command::Gather{item:"workshop:ore".into(),amount:6}}],
 8=>vec![Envelope{seat:0,serial:3,command:Command::Cancel{machine:"workshop:machine-a".into()}}],
 12=>vec![Envelope{seat:0,serial:4,command:Command::Deliver{item:"workshop:ore".into(),amount:3}}],
 _=>vec![]}).collect()}
#[test]fn deterministic_public_player_commands(){assert_deterministic(Workshop::standard,&inputs());}
#[test]fn saves_resume_exactly_during_production_and_blocked_refund(){let input=inputs();for at in [1,3,9,13,45]{snapshot::assert_resumes_exactly(Workshop::standard,&input,at);}}
#[test]fn whole_input_batch_limits_do_not_partially_charge(){let mut sim=Workshop::standard();let before=sim.state.inventory.clone();sim.step(&vec![Envelope{seat:0,serial:1,command:Command::Gather{item:"workshop:ore".into(),amount:1}};33]);assert_eq!(sim.state.inventory,before);assert_eq!(sim.state.tick,1);assert!(sim.outcomes[0].detail.contains("COMMAND_LIMIT"));}
