//! Generate an interactive HTML trace visualization.
//!
//! Usage:
//!   cargo run --example visualize
//!   open target/visualize.html

#[path = "scenarios/mod.rs"]
mod scenarios;

use two_phase_commit::simulator::{ExternalEvent, LogEntry, Simulator};
use two_phase_commit::types::*;

fn msg_str(mt: &MessageType) -> &'static str {
    match mt {
        MessageType::StartTransaction => "StartTransaction",
        MessageType::Prepare => "Prepare",
        MessageType::VoteCommit => "VoteCommit",
        MessageType::VoteAbort => "VoteAbort",
        MessageType::DecisionCommit => "DecisionCommit",
        MessageType::DecisionAbort => "DecisionAbort",
        MessageType::Ack => "Ack",
    }
}

fn decision_json(d: Option<Decision>) -> &'static str {
    match d {
        Some(Decision::Commit) => "\"Commit\"",
        Some(Decision::Abort) => "\"Abort\"",
        None => "null",
    }
}

fn vote_json(v: Option<Vote>) -> &'static str {
    match v {
        Some(Vote::Commit) => "\"Commit\"",
        Some(Vote::Abort) => "\"Abort\"",
        None => "null",
    }
}

fn entry_json(e: &LogEntry) -> Option<String> {
    match e {
        LogEntry::ExternalEvent { at, event } => match event {
            ExternalEvent::StartTransaction => Some(format!(
                r#"{{"kind":"event","at":{at},"actor":"Coordinator","label":"StartTransaction"}}"#
            )),
            ExternalEvent::Crash(actor) => {
                Some(format!(r#"{{"kind":"crash","at":{at},"actor":"{actor}"}}"#))
            }
            ExternalEvent::Recover(actor) => Some(format!(
                r#"{{"kind":"recover","at":{at},"actor":"{actor}"}}"#
            )),
            ExternalEvent::TickAll | ExternalEvent::Tick { .. } => None,
        },
        LogEntry::Send {
            at,
            deliver_at,
            msg,
        } => Some(format!(
            r#"{{"kind":"send","at":{at},"deliver_at":{deliver_at},"from":"{f}","to":"{t}","msg":"{m}"}}"#,
            f = msg.from,
            t = msg.to,
            m = msg_str(&msg.message_type)
        )),
        LogEntry::Deliver { at, msg } => Some(format!(
            r#"{{"kind":"deliver","at":{at},"from":"{f}","to":"{t}","msg":"{m}"}}"#,
            f = msg.from,
            t = msg.to,
            m = msg_str(&msg.message_type)
        )),
        LogEntry::Drop { at, msg } => Some(format!(
            r#"{{"kind":"drop","at":{at},"from":"{f}","to":"{t}","msg":"{m}"}}"#,
            f = msg.from,
            t = msg.to,
            m = msg_str(&msg.message_type)
        )),
    }
}

fn scenario_json(name: &str, sim: &Simulator) -> String {
    let mut actors = vec!["Coordinator".to_string()];
    for id in sim.participants().keys() {
        actors.push(format!("{}", ActorId::Node(*id)));
    }
    let actor_list = actors
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(",");
    let entries: Vec<String> = sim.log().iter().filter_map(entry_json).collect();

    let coord_dec = decision_json(sim.coordinator().decision());
    let parts: Vec<String> = sim
        .participants()
        .iter()
        .map(|(id, p)| {
            format!(
                r#"{{"id":"{}","decision":{},"vote":{}}}"#,
                ActorId::Node(*id),
                decision_json(p.decision()),
                vote_json(p.vote()),
            )
        })
        .collect();

    format!(
        r#"{{"name":"{name}","actors":[{actor_list}],"entries":[{e}],"result":{{"coordinator":{coord_dec},"participants":[{p}]}}}}"#,
        e = entries.join(","),
        p = parts.join(",")
    )
}

fn main() {
    let all = scenarios::all();
    let jsons: Vec<String> = all.iter().map(|s| scenario_json(s.name, &s.sim)).collect();
    let json = format!("[{}]", jsons.join(",\n"));
    let html = HTML_TEMPLATE.replace("__DATA__", &json);

    std::fs::create_dir_all("target").ok();
    let path = std::path::Path::new("target/visualize.html");
    std::fs::write(path, &html).expect("Failed to write HTML");

    let abs = std::fs::canonicalize(path).unwrap();
    eprintln!("Written to {}", abs.display());

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&abs).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&abs).spawn();
    }
}

const HTML_TEMPLATE: &str = include_str!("visualize.html");
