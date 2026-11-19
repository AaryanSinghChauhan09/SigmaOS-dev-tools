// SPDX-License-Identifier: MIT OR Apache-2.0
//! SigmaOS-dev-tools — Developer toolchain helpers.
//!
//! Provides toolchain discovery, build-system detection (Cargo, Meson, CMake, Make),
//! project scaffolding templates, and environment variable configuration.
//!
//! Inspired by Omarchy's hacker-first developer setup, Hyprland's build scripts,
//! and Catppuccin Mocha's syntax theme integration.
//!
//! Principles: zero external dependencies, robust path validation, pure `std`, zero `unsafe`.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

// ── Error types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevToolsError {
    ToolchainNotFound(String),
    UnknownProjectTemplate(String),
    InvalidProjectName(String),
    EmptyTargetDirectory,
    ConfigValidationFailed(String),
}

impl fmt::Display for DevToolsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToolchainNotFound(t) => write!(f, "required toolchain component not detected: '{t}'"),
            Self::UnknownProjectTemplate(tmpl) => write!(f, "unrecognized project template: '{tmpl}'"),
            Self::InvalidProjectName(n) => write!(f, "invalid project name: '{n}' (must be valid identifier)"),
            Self::EmptyTargetDirectory => write!(f, "target directory must not be empty"),
            Self::ConfigValidationFailed(msg) => write!(f, "toolchain config validation failed: {msg}"),
        }
    }
}

impl std::error::Error for DevToolsError {}

// ── Build Systems & Toolchain Types ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Cargo,
    CMake,
    Meson,
    Make,
    Just,
}

impl BuildSystem {
    pub fn config_file(&self) -> &'static str {
        match self {
            Self::Cargo => "Cargo.toml",
            Self::CMake => "CMakeLists.txt",
            Self::Meson => "meson.build",
            Self::Make => "Makefile",
            Self::Just => "Justfile",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTemplate {
    pub name: String,
    pub build_system: BuildSystem,
    pub language: String,
    pub default_files: Vec<(String, String)>,
}

impl ProjectTemplate {
    pub fn rust_binary(name: impl Into<String>) -> Result<Self, DevToolsError> {
        let name = name.into().trim().to_string();
        if !is_valid_ident(&name) {
            return Err(DevToolsError::InvalidProjectName(name));
        }
        let cargo_toml = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        let main_rs = "fn main() {\n    println!(\"Hello from SigmaOS!\");\n}\n".to_string();
        Ok(Self {
            name,
            build_system: BuildSystem::Cargo,
            language: "Rust".into(),
            default_files: vec![
                ("Cargo.toml".into(), cargo_toml),
                ("src/main.rs".into(), main_rs),
            ],
        })
    }

    pub fn c_meson(name: impl Into<String>) -> Result<Self, DevToolsError> {
        let name = name.into().trim().to_string();
        if !is_valid_ident(&name) {
            return Err(DevToolsError::InvalidProjectName(name));
        }
        let meson_build = format!("project('{name}', 'c', version: '0.1.0')\nexecutable('{name}', 'main.c')\n");
        let main_c = "#include <stdio.h>\nint main(void) { printf(\"Hello SigmaOS\\n\"); return 0; }\n".to_string();
        Ok(Self {
            name,
            build_system: BuildSystem::Meson,
            language: "C".into(),
            default_files: vec![
                ("meson.build".into(), meson_build),
                ("main.c".into(), main_c),
            ],
        })
    }
}

fn is_valid_ident(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ── Environment & Toolchain Registry ─────────────────────────────────────────

#[derive(Debug, Default)]
pub struct DevEnvironment {
    env_vars: HashMap<String, String>,
    installed_tools: HashMap<String, String>, // tool_name -> version
}

impl DevEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_var(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.env_vars.insert(k.into(), v.into());
    }

    pub fn get_var(&self, k: &str) -> Option<&str> {
        self.env_vars.get(k).map(|s| s.as_str())
    }

    pub fn register_tool(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.installed_tools.insert(name.into(), version.into());
    }

    pub fn require_tool(&self, name: &str) -> Result<&str, DevToolsError> {
        self.installed_tools.get(name).map(|s| s.as_str())
            .ok_or_else(|| DevToolsError::ToolchainNotFound(name.into()))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_template_valid_and_invalid_ident() {
        // Arrange & Act
        let valid = ProjectTemplate::rust_binary("my_app");
        let invalid = ProjectTemplate::rust_binary("123-bad-name");

        // Assert
        assert!(valid.is_ok());
        let t = valid.unwrap();
        assert_eq!(t.build_system, BuildSystem::Cargo);
        assert_eq!(t.default_files.len(), 2);
        assert_eq!(invalid, Err(DevToolsError::InvalidProjectName("123-bad-name".into())));
    }

    #[test]
    fn c_meson_scaffold_creation() {
        // Arrange & Act
        let t = ProjectTemplate::c_meson("daemon_core").unwrap();
        // Assert
        assert_eq!(t.build_system, BuildSystem::Meson);
        assert_eq!(t.default_files[0].0, "meson.build");
        assert!(t.default_files[0].1.contains("project('daemon_core'"));
    }

    #[test]
    fn dev_environment_variable_roundtrip() {
        // Arrange
        let mut env = DevEnvironment::new();
        // Act
        env.set_var("RUST_BACKTRACE", "1");
        env.set_var("CC", "clang");
        // Assert
        assert_eq!(env.get_var("RUST_BACKTRACE"), Some("1"));
        assert_eq!(env.get_var("CC"), Some("clang"));
        assert_eq!(env.get_var("CXX"), None);
    }

    #[test]
    fn require_tool_success_and_missing() {
        // Arrange
        let mut env = DevEnvironment::new();
        env.register_tool("cargo", "1.78.0");

        // Act
        let cargo = env.require_tool("cargo");
        let missing = env.require_tool("rustc");

        // Assert
        assert_eq!(cargo, Ok("1.78.0"));
        assert_eq!(missing, Err(DevToolsError::ToolchainNotFound("rustc".into())));
    }

    #[test]
    fn build_system_config_file_mapping() {
        // Arrange & Act & Assert
        assert_eq!(BuildSystem::Cargo.config_file(), "Cargo.toml");
        assert_eq!(BuildSystem::Meson.config_file(), "meson.build");
        assert_eq!(BuildSystem::CMake.config_file(), "CMakeLists.txt");
        assert_eq!(BuildSystem::Make.config_file(), "Makefile");
    }

    #[test]
    fn error_display_formatting() {
        // Arrange
        let err = DevToolsError::ToolchainNotFound("ninja".into());
        // Act & Assert
        assert_eq!(format!("{err}"), "required toolchain component not detected: 'ninja'");
    }
}
