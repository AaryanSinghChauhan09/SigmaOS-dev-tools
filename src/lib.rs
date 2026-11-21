// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-dev-tools — development environment/toolchain manager.
//!
//! Manages dev toolchains (rust, go, node, python), versions, per-project
//! env selection, and PATH/env rendering. JSON persistence for installed
//! toolchains; a tool registry with install status; environment-file
//! renderer for shell init.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolKind { Runtime, Lsp, Build, Vcs, Editor, Debugger, Misc }
impl ToolKind {
    pub fn label(self) -> &'static str {
        match self { Self::Runtime=>"runtime", Self::Lsp=>"lsp", Self::Build=>"build", Self::Vcs=>"vcs", Self::Editor=>"editor", Self::Debugger=>"debugger", Self::Misc=>"misc" }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tool {
    pub name: String,
    pub kind: ToolKind,
    pub installed: bool,
    pub version: Option<String>,
    pub bin_dir: Option<PathBuf>,   // where its binaries live
    pub default_version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Toolchain {
    pub name: String,               // e.g. "rust-stable"
    pub tool: String,               // tool name it belongs to
    pub version: String,
    pub path: PathBuf,              // toolchain root
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DevError {
    NoSuchTool(String),
    NoSuchToolchain(String),
    AlreadyInstalled(String),
    NotInstalled(String),
    NoDefault(String),
    BadVersion(String),
}
impl fmt::Display for DevError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchTool(n)=>write!(f,"no tool {n}"),
            Self::NoSuchToolchain(n)=>write!(f,"no toolchain {n}"),
            Self::AlreadyInstalled(n)=>write!(f,"{n} already installed"),
            Self::NotInstalled(n)=>write!(f,"{n} not installed"),
            Self::NoDefault(n)=>write!(f,"no default toolchain for {n}"),
            Self::BadVersion(v)=>write!(f,"bad version {v}"),
        }
    }
}
impl std::error::Error for DevError {}

/// A project env: dir -> tool@version selections.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectEnv {
    pub dir: PathBuf,
    pub selections: BTreeMap<String, String>, // tool -> toolchain name
}

pub struct SigmaOSDevTools {
    tools: BTreeMap<String, Tool>,
    toolchains: BTreeMap<String, Toolchain>,
    projects: Vec<ProjectEnv>,
}

impl SigmaOSDevTools {
    pub fn new() -> Self {
        let mut m = Self { tools: BTreeMap::new(), toolchains: BTreeMap::new(), projects: Vec::new() };
        m.seed_tools();
        m
    }

    fn seed_tools(&mut self) {
        let defs = vec![
            ("rust", ToolKind::Runtime), ("cargo", ToolKind::Build),
            ("go", ToolKind::Runtime), ("node", ToolKind::Runtime), ("npm", ToolKind::Runtime),
            ("python3", ToolKind::Runtime), ("pip", ToolKind::Runtime),
            ("rust-analyzer", ToolKind::Lsp), ("pyright", ToolKind::Lsp),
            ("clangd", ToolKind::Lsp), ("gopls", ToolKind::Lsp),
            ("cmake", ToolKind::Build), ("ninja", ToolKind::Build), ("make", ToolKind::Build),
            ("git", ToolKind::Vcs), ("gh", ToolKind::Vcs), ("lldb", ToolKind::Debugger),
            ("nvim", ToolKind::Editor), ("code", ToolKind::Editor),
            ("docker", ToolKind::Misc), ("podman", ToolKind::Misc),
        ];
        for (name, kind) in defs {
            // probe the real host for what's installed
            let (installed, version) = probe_tool(name);
            self.tools.insert(name.into(), Tool {
                name: name.into(), kind, installed,
                version, bin_dir: None, default_version: None,
            });
        }
    }

    // ---- registry ----
    pub fn tool(&self, name: &str) -> Option<&Tool> { self.tools.get(name) }
    pub fn tools(&self) -> impl Iterator<Item = &Tool> { self.tools.values() }
    pub fn tools_of(&self, kind: ToolKind) -> Vec<&Tool> {
        self.tools.values().filter(|t| t.kind == kind).collect()
    }
    pub fn installed(&self) -> Vec<&Tool> { self.tools.values().filter(|t| t.installed).collect() }
    pub fn missing(&self) -> Vec<&Tool> { self.tools.values().filter(|t| !t.installed).collect() }

    // ---- toolchains ----
    pub fn add_toolchain(&mut self, tc: Toolchain) -> Result<(), DevError> {
        if self.toolchains.contains_key(&tc.name) {
            return Err(DevError::AlreadyInstalled(tc.name.clone()));
        }
        // first toolchain for a tool becomes default
        let is_first = !self.toolchains.values().any(|t| t.tool == tc.tool);
        let tc = Toolchain { default: is_first, ..tc };
        self.toolchains.insert(tc.name.clone(), tc); Ok(())
    }

    pub fn toolchain(&self, name: &str) -> Option<&Toolchain> { self.toolchains.get(name) }
    pub fn toolchains(&self) -> impl Iterator<Item = &Toolchain> { self.toolchains.values() }
    pub fn toolchains_of(&self, tool: &str) -> Vec<&Toolchain> {
        self.toolchains.values().filter(|t| t.tool == tool).collect()
    }

    pub fn default_toolchain(&self, tool: &str) -> Result<&Toolchain, DevError> {
        self.toolchains_of(tool).into_iter().find(|t| t.default)
            .ok_or_else(|| DevError::NoDefault(tool.into()))
    }

    pub fn set_default(&mut self, name: &str) -> Result<(), DevError> {
        let tool = self.toolchains.get(name).ok_or_else(|| DevError::NoSuchToolchain(name.into()))?.tool.clone();
        for t in self.toolchains.values_mut() {
            if t.tool == tool { t.default = t.name == name; }
        }
        Ok(())
    }

    pub fn remove_toolchain(&mut self, name: &str) -> Result<(), DevError> {
        let was_default = self.toolchains.get(name).map(|t| t.default).unwrap_or(false);
        let tool = self.toolchains.get(name).ok_or_else(|| DevError::NoSuchToolchain(name.into()))?.tool.clone();
        self.toolchains.remove(name);
        // promote another toolchain of the same tool if the default was removed
        if was_default {
            if let Some(first) = self.toolchains.values_mut().find(|t| t.tool == tool) {
                first.default = true;
            }
        }
        Ok(())
    }

    // ---- projects ----
    pub fn set_project(&mut self, dir: PathBuf, tool: &str, toolchain: &str) -> Result<(), DevError> {
        if !self.tools.contains_key(tool) { return Err(DevError::NoSuchTool(tool.into())); }
        if !self.toolchains.contains_key(toolchain) {
            return Err(DevError::NoSuchToolchain(toolchain.into()));
        }
        if let Some(p) = self.projects.iter_mut().find(|p| p.dir == dir) {
            p.selections.insert(tool.into(), toolchain.into());
        } else {
            let mut sel = BTreeMap::new();
            sel.insert(tool.into(), toolchain.into());
            self.projects.push(ProjectEnv { dir, selections: sel });
        }
        Ok(())
    }

    pub fn project(&self, dir: &Path) -> Option<&ProjectEnv> {
        self.projects.iter().find(|p| p.dir == dir)
    }
    pub fn projects(&self) -> &[ProjectEnv] { &self.projects }

    // ---- PATH/env rendering ----
    /// PATH additions for all default toolchains.
    pub fn path_additions(&self) -> Vec<PathBuf> {
        self.toolchains.values().filter(|t| t.default).map(|t| t.path.join("bin")).collect()
    }

    /// Render shell env exports for a project dir (or defaults).
    pub fn render_env(&self, project_dir: Option<&Path>) -> String {
        let mut out = String::new();
        let mut seen = std::collections::BTreeSet::new();
        // project-specific selections first
        if let Some(dir) = project_dir {
            if let Some(p) = self.project(dir) {
                for (tool, tc_name) in &p.selections {
                    if let Some(tc) = self.toolchains.get(tc_name) {
                        let v = format!("{}_HOME", tool.to_uppercase().replace('-', "_"));
                        out.push_str(&format!("export {}={}\n", v, tc.path.display()));
                        seen.insert(tc.path.join("bin"));
                    }
                }
            }
        }
        for tc in self.toolchains.values().filter(|t| t.default) {
            let bin = tc.path.join("bin");
            if seen.insert(bin.clone()) {
                out.push_str(&format!("export PATH=\"{}:$PATH\"\n", bin.display()));
            }
        }
        out
    }

    /// Persist toolchains + project selections to JSON.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Disk {
            toolchains: Vec<Toolchain>, projects: Vec<ProjectEnv>,
        }
        let d = Disk { toolchains: self.toolchains.values().cloned().collect(),
                       projects: self.projects.clone() };
        let j = serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?;
        if let Some(p) = path.parent() { std::fs::create_dir_all(p).map_err(|e| e.to_string())?; }
        std::fs::write(path, j).map_err(|e| e.to_string())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self, String> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Disk {
            toolchains: Vec<Toolchain>, projects: Vec<ProjectEnv>,
        }
        let txt = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let d: Disk = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        let mut m = Self::new();
        let defaults: Vec<String> = d.toolchains.iter().filter(|t| t.default).map(|t| t.name.clone()).collect();
        for tc in d.toolchains { let _ = m.add_toolchain(tc); }
        // restore persisted default flags
        for name in defaults { let _ = m.set_default(&name); }
        m.projects = d.projects;
        Ok(m)
    }

    pub fn state_path() -> std::path::PathBuf {
        if let Ok(p) = std::env::var("SIGMAOS_DEVTOOLS_FILE") { return std::path::PathBuf::from(p); }
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        std::path::PathBuf::from(home).join(".config/sigmaos/dev-tools.json")
    }

    /// Status summary.
    pub fn status_line(&self) -> String {
        format!("dev-tools: {}/{} installed, {} toolchains, {} projects",
            self.installed().len(), self.tools.len(), self.toolchains.len(), self.projects.len())
    }
}

impl Default for SigmaOSDevTools { fn default() -> Self { Self::new() } }

/// Probe the real host: `which` + `--version`.
fn probe_tool(name: &str) -> (bool, Option<String>) {
    // which
    let which = std::process::Command::new("which").arg(name).output();
    let Ok(w) = which else { return (false, None) };
    if !w.status.success() { return (false, None); }
    let path = String::from_utf8_lossy(&w.stdout).trim().to_string();
    if path.is_empty() { return (false, None); }
    // version: try --version then -version
    for flag in ["--version", "-version", "version"] {
        if let Ok(o) = std::process::Command::new(&path).arg(flag).output() {
            if o.status.success() {
                let so = String::from_utf8_lossy(&o.stdout).to_string();
                let se = String::from_utf8_lossy(&o.stderr).to_string();
                let v = so.lines().next().or_else(|| se.lines().next()).unwrap_or("").trim().to_string();
                if !v.is_empty() { return (true, Some(v)); }
            }
        }
    }
    (true, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc(name: &str, tool: &str, ver: &str, default: bool) -> Toolchain {
        Toolchain { name: name.into(), tool: tool.into(), version: ver.into(),
            path: PathBuf::from(format!("/opt/{}", name)), default }
    }

    #[test]
    fn seeds_and_probes() {
        let m = SigmaOSDevTools::new();
        // at least some tools should exist in registry
        assert!(m.tools().count() >= 15);
        // git/python3 likely installed on host
        assert!(m.tool("git").is_some());
        // tool kind filter works
        assert!(!m.tools_of(ToolKind::Lsp).is_empty());
    }

    #[test]
    fn toolchain_lifecycle() {
        let mut m = SigmaOSDevTools::new();
        m.add_toolchain(tc("rust-stable", "rust", "1.75", false)).unwrap();
        // first toolchain auto-defaults
        assert!(m.toolchain("rust-stable").unwrap().default);
        m.add_toolchain(tc("rust-nightly", "rust", "nightly", false)).unwrap();
        assert!(!m.toolchain("rust-nightly").unwrap().default);
        m.set_default("rust-nightly").unwrap();
        assert!(m.toolchain("rust-nightly").unwrap().default);
        assert!(!m.toolchain("rust-stable").unwrap().default);
        assert!(m.add_toolchain(tc("rust-stable", "rust", "1.0", false)).is_err()); // dup
    }

    #[test]
    fn default_toolchain_lookup() {
        let mut m = SigmaOSDevTools::new();
        m.add_toolchain(tc("go1", "go", "1.21", false)).unwrap();
        m.add_toolchain(tc("go2", "go", "1.22", false)).unwrap();
        m.set_default("go2").unwrap();
        assert_eq!(m.default_toolchain("go").unwrap().name, "go2");
        assert!(m.default_toolchain("rust").is_err()); // none
    }

    #[test]
    fn project_env() {
        let mut m = SigmaOSDevTools::new();
        m.add_toolchain(tc("rust-stable", "rust", "1.75", false)).unwrap();
        let dir = PathBuf::from("/home/u/proj");
        m.set_project(dir.clone(), "rust", "rust-stable").unwrap();
        let p = m.project(&dir).unwrap();
        assert_eq!(p.selections["rust"], "rust-stable");
        // unknown tool errors
        assert!(m.set_project(dir.clone(), "bogus", "rust-stable").is_err());
        // unknown toolchain errors
        assert!(m.set_project(dir, "rust", "nope").is_err());
    }

    #[test]
    fn env_rendering() {
        let mut m = SigmaOSDevTools::new();
        m.add_toolchain(tc("rust-stable", "rust", "1.75", false)).unwrap();
        let env = m.render_env(None);
        assert!(env.contains("export PATH="));
        assert!(env.contains("rust-stable/bin"));
        // project-specific adds TOOL_HOME exports
        let dir = PathBuf::from("/proj");
        m.set_project(dir.clone(), "rust", "rust-stable").unwrap();
        let env2 = m.render_env(Some(&dir));
        assert!(env2.contains("export RUST_HOME="));
    }

    #[test]
    fn status_and_kinds() {
        let m = SigmaOSDevTools::new();
        let s = m.status_line();
        assert!(s.contains("dev-tools:"));
        assert!(m.tools_of(ToolKind::Vcs).iter().any(|t| t.name == "git"));
        assert!(m.tools_of(ToolKind::Editor).iter().any(|t| t.name == "nvim"));
    }
}
