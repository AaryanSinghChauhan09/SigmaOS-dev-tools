//! SigmaOS-dev-tools — dev toolchain CLI.
//!
//! Usage:
//!   sigmaos-dev-tools status|installed|missing|tools|kinds
//!   sigmaos-dev-tools toolchain add <name> <tool> <version> [path]
//!   sigmaos-dev-tools toolchain list|default <tool>|set-default <name>|remove <name>
//!   sigmaos-dev-tools project set <dir> <tool> <toolchain>
//!   sigmaos-dev-tools project list|env [dir]
//!   sigmaos-dev-tools path                          print PATH additions

use dev_tools::{SigmaOSDevTools, Toolchain, ToolKind};
use std::path::PathBuf;
use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: sigmaos-dev-tools <status|installed|missing|tools|kinds|toolchain|project|path> [args]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); }
    let state = SigmaOSDevTools::state_path();
    let mut m = SigmaOSDevTools::load_from(&state).unwrap_or_else(|_| SigmaOSDevTools::new());
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let arg = |i: usize| -> &str { rest.get(i).map(|s| s.as_str()).unwrap_or_else(|| usage()) };

    let result: Result<(), String> = (|| -> Result<(), String> { match cmd {
        "status" => { println!("{}", m.status_line()); Ok(()) }
        "installed" => {
            for t in m.installed() { println!("{:<16} {:<10} {}", t.name, t.kind.label(), t.version.as_deref().unwrap_or("?")); }
            Ok(())
        }
        "missing" => {
            for t in m.missing() { println!("{:<16} {}", t.name, t.kind.label()); }
            Ok(())
        }
        "tools" => {
            for t in m.tools() { println!("{} {:<16} {:<10} {}", if t.installed {"*"} else {" "}, t.name, t.kind.label(), t.version.as_deref().unwrap_or("")); }
            Ok(())
        }
        "kinds" => {
            for k in [ToolKind::Runtime, ToolKind::Lsp, ToolKind::Build, ToolKind::Vcs, ToolKind::Editor, ToolKind::Debugger, ToolKind::Misc] {
                println!("{:<10} {}", k.label(), m.tools_of(k).len());
            }
            Ok(())
        }
        "toolchain" => {
            match rest.first().map(|s| s.as_str()).unwrap_or_else(|| usage()) {
                "add" => {
                    let tc = Toolchain { name: arg(1).into(), tool: arg(2).into(), version: arg(3).into(),
                        path: rest.get(4).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("/opt/{}", arg(1)))),
                        default: false };
                    m.add_toolchain(tc).map_err(|e| e.to_string())?;
                    println!("added toolchain {}", arg(1)); Ok(())
                }
                "list" => {
                    for t in m.toolchains() { println!("{} {:<14} {:<8} {}{}", if t.default {"*"} else {" "}, t.name, t.tool, t.version, t.path.display()); }
                    Ok(())
                }
                "default" => {
                    let tc = m.default_toolchain(arg(1)).map_err(|e| e.to_string())?;
                    println!("{} {}", tc.name, tc.version); Ok(())
                }
                "set-default" => { m.set_default(arg(1)).map_err(|e| e.to_string())?; println!("default={}", arg(1)); Ok(()) }
                "remove" => { m.remove_toolchain(arg(1)).map_err(|e| e.to_string())?; println!("removed {}", arg(1)); Ok(()) }
                _ => usage(),
            }
        }
        "project" => {
            match rest.first().map(|s| s.as_str()).unwrap_or_else(|| usage()) {
                "set" => {
                    m.set_project(PathBuf::from(arg(1)), arg(2), arg(3)).map_err(|e| e.to_string())?;
                    println!("project {}: {}={}", arg(1), arg(2), arg(3)); Ok(())
                }
                "list" => {
                    for p in m.projects() {
                        let sels: Vec<String> = p.selections.iter().map(|(t,c)| format!("{}={}", t, c)).collect();
                        println!("{}: {}", p.dir.display(), sels.join(", "));
                    }
                    Ok(())
                }
                "env" => {
                    let dir = rest.get(1).map(PathBuf::from);
                    print!("{}", m.render_env(dir.as_deref())); Ok(())
                }
                _ => usage(),
            }
        }
        "path" => {
            for p in m.path_additions() { println!("{}", p.display()); }
            Ok(())
        }
        _ => usage(),
    } })();

    match result {
        Ok(()) => { if let Err(e) = m.save_to(&state) { eprintln!("sigmaos-dev-tools: warn: save: {e}"); } }
        Err(e) => { eprintln!("sigmaos-dev-tools: {e}"); exit(1); }
    }
}
