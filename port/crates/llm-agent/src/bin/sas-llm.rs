// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas-llm`: chat with the synth's tools from a terminal, on a preset file.
//!
//! The API key comes from the provider's environment variable
//! (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) or `SAS_LLM_API_KEY`; it is never
//! written anywhere. `--record` saves the exchanges (without headers) for
//! replay tests; `--replay` plays one back without a network.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use sas_llm_agent::eval::{self, EvalCase};
use sas_llm_agent::{
    Agent, AgentConfig, AgentEvent, HttpTransport, ProviderConfig, ProviderKind,
    RecordingTransport, ReplayTransport, Transport,
};
use sas_llm_control::sas_preset::{InitContent, PresetState};
use sas_llm_control::{patch, Controller, OfflineSession, Policy, PresetLibrary, SynthSession};

const USAGE: &str = "\
usage: sas-llm [options]

Chat with the synth's tools on a preset. Type a request; commands:
  /patch   show the current patch      /undo [n]  undo n steps (default 1)
  /yes     confirm a held change       /no        decline it
  /usage   tokens used so far          /quit      exit (also end of input)

options:
  --provider NAME    anthropic (default) or openai (any OpenAI-compatible server)
  --model ID         model id (default: the provider's default)
  --base-url URL     API base URL (e.g. http://localhost:11434/v1 for a local server)
  --preset FILE      start from this .vital preset (default: the init patch)
  --preset-dir DIR   folder the preset tools may read and write
  --save FILE        write the final patch to this .vital file on exit
  --prompt TEXT      send TEXT instead of reading stdin (repeatable)
  --record FILE      record the exchanges to FILE (.jsonl, no headers or keys)
  --replay FILE      replay a recording instead of calling the provider
  --eval FILE        run an eval set (JSON) and report which checks pass
";

struct Options {
    provider: ProviderKind,
    model: Option<String>,
    base_url: Option<String>,
    preset: Option<PathBuf>,
    preset_dir: Option<PathBuf>,
    save: Option<PathBuf>,
    prompts: Vec<String>,
    record: Option<PathBuf>,
    replay: Option<PathBuf>,
    eval: Option<PathBuf>,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        provider: ProviderKind::Anthropic,
        model: None,
        base_url: None,
        preset: None,
        preset_dir: None,
        save: None,
        prompts: Vec::new(),
        record: None,
        replay: None,
        eval: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--provider" => {
                let name = value()?;
                options.provider =
                    ProviderKind::parse(&name).ok_or_else(|| format!("unknown provider {name}"))?;
            }
            "--model" => options.model = Some(value()?),
            "--base-url" => options.base_url = Some(value()?),
            "--preset" => options.preset = Some(value()?.into()),
            "--preset-dir" => options.preset_dir = Some(value()?.into()),
            "--save" => options.save = Some(value()?.into()),
            "--prompt" => options.prompts.push(value()?),
            "--record" => options.record = Some(value()?.into()),
            "--replay" => options.replay = Some(value()?.into()),
            "--eval" => options.eval = Some(value()?.into()),
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    Ok(options)
}

fn provider(options: &Options) -> ProviderConfig {
    let key = std::env::var(options.provider.key_variable())
        .or_else(|_| std::env::var("SAS_LLM_API_KEY"))
        .ok()
        .filter(|key| !key.is_empty());
    let mut config = ProviderConfig::new(options.provider, key);
    if let Some(model) = &options.model {
        config.model = model.clone();
    }
    if let Some(url) = &options.base_url {
        config.base_url = url.clone();
    }
    config
}

fn transport(options: &Options) -> Result<Box<dyn Transport>, String> {
    if let Some(path) = &options.replay {
        return Ok(Box::new(
            ReplayTransport::open(path).map_err(|e| e.to_string())?,
        ));
    }
    let http = HttpTransport::new().map_err(|e| e.to_string())?;
    match &options.record {
        Some(path) => Ok(Box::new(
            RecordingTransport::new(http, path).map_err(|e| e.to_string())?,
        )),
        None => Ok(Box::new(http)),
    }
}

fn initial_state(options: &Options) -> Result<PresetState, String> {
    let mut state = PresetState::new(&InitContent { sample_seed: 0 });
    if let Some(path) = &options.preset {
        state
            .load_file(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(state)
}

fn agent(options: &Options, state: PresetState) -> Result<Agent<OfflineSession>, String> {
    let mut controller = Controller::new(OfflineSession::new(state), Policy::default());
    if let Some(dir) = &options.preset_dir {
        controller = controller.with_library(PresetLibrary::new(dir));
    }
    Ok(Agent::new(
        controller,
        provider(options).build(),
        transport(options)?,
        AgentConfig::default(),
    ))
}

fn print_event(event: AgentEvent) {
    match event {
        AgentEvent::Text(text) => println!("{text}"),
        AgentEvent::ToolCall { name, input } => {
            let mut args = input.to_string();
            if args.len() > 160 {
                args.truncate(157);
                args.push_str("...");
            }
            println!("  > {name} {args}");
        }
        AgentEvent::ToolResult { output, .. } if output.is_error => {
            println!("  ! {}", output.content);
        }
        AgentEvent::ToolResult { .. } => {}
    }
}

fn send(agent: &mut Agent<OfflineSession>, text: &str) {
    match agent.send(text, &mut print_event) {
        Ok(report) => {
            if let Some(stopped) = report.stopped {
                println!("({stopped})");
            }
            if let Some(pending) = agent.controller().pending() {
                println!(
                    "(held for your confirmation: {}; /yes or /no)",
                    pending.reason
                );
            }
        }
        Err(error) => eprintln!("error: {error}"),
    }
}

fn chat(options: &Options) -> Result<(), String> {
    let mut agent = agent(options, initial_state(options)?)?;
    if options.prompts.is_empty() {
        let stdin = std::io::stdin();
        print!("> ");
        std::io::stdout().flush().ok();
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            let line = line.trim();
            let mut words = line.split_whitespace();
            match words.next() {
                None => {}
                Some("/quit") => break,
                Some("/patch") => {
                    print!(
                        "{}",
                        patch::describe(&agent.controller().session().snapshot())
                    )
                }
                Some("/undo") => {
                    let steps = words.next().and_then(|n| n.parse().ok()).unwrap_or(1);
                    match agent.controller_mut().undo(steps, None) {
                        Ok(count) => {
                            agent.note_user_undo(count);
                            println!("(undid {count})");
                        }
                        Err(error) => eprintln!("error: {error}"),
                    }
                }
                Some(answer @ ("/yes" | "/no")) => match agent.resolve_pending(answer == "/yes") {
                    Ok(true) => println!(
                        "({})",
                        if answer == "/yes" {
                            "applied"
                        } else {
                            "declined"
                        }
                    ),
                    Ok(false) => println!("(nothing is waiting)"),
                    Err(error) => eprintln!("error: {error}"),
                },
                Some("/usage") => println!("{:?}", agent.usage()),
                Some(_) => send(&mut agent, line),
            }
            print!("> ");
            std::io::stdout().flush().ok();
        }
    } else {
        for prompt in &options.prompts {
            println!("> {prompt}");
            send(&mut agent, prompt);
        }
    }
    if let Some(path) = &options.save {
        std::fs::write(path, agent.controller().session().state().to_json_string())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("saved {}", path.display());
    }
    Ok(())
}

fn run_eval(options: &Options, path: &PathBuf) -> Result<bool, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cases: Vec<EvalCase> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let init = initial_state(options)?;
    let mut passed = 0;
    for case in &cases {
        let mut agent = agent(options, init.clone())?;
        let result = agent.send(&case.prompt, &mut |_| {});
        let failures: Vec<String> = match result {
            Err(error) => vec![error.to_string()],
            Ok(_) => {
                let state = agent.controller().session().snapshot();
                case.checks
                    .iter()
                    .filter_map(|check| eval::check(&state, &init, check).err())
                    .collect()
            }
        };
        if failures.is_empty() {
            passed += 1;
            println!("pass  {}", case.name);
        } else {
            println!("FAIL  {}: {}", case.name, failures.join("; "));
        }
    }
    println!("{passed} of {} passed", cases.len());
    Ok(passed == cases.len())
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("{error}");
            }
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match &options.eval {
        Some(path) => run_eval(&options, path).map(|all| if all { 0 } else { 1 }),
        None => chat(&options).map(|_| 0),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
