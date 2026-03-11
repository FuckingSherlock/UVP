use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const GREEN: &str = "\x1b[0;32m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const BLUE: &str = "\x1b[0;34m";
const NC: &str = "\x1b[0m";
const DEFAULT_VERSION: &str = "3.10";

struct Config {
    force_yes: bool,
    force_no: bool,
    command: String,
    version: String,
}

fn main() {
    let config = match parse_args() {
        Some(c) => c,
        None => {
            print_help();
            return;
        }
    };

    match config.command.as_str() {
        "init" => do_init(config),
        "pin" => {
            if config.version.is_empty() {
                println!("{}❌ Error: version required{}", RED, NC);
                return;
            }
            println!("{}🔄 Pinning Python {}...{}", GREEN, config.version, NC);
            run_uv(&["python", "pin", &config.version]);
            run_uv(&["sync"]);
        }
        "update" => {
            if config.version.is_empty() {
                println!("{}❌ Error: version required{}", RED, NC);
                return;
            }
            println!("{}🌍 Updating to Python {}...{}", GREEN, config.version, NC);
            if let Err(e) = update_toml(&config.version) {
                println!("{}❌ File error: {}{}", RED, e, NC);
            }
            run_uv(&["python", "pin", &config.version]);
            run_uv(&["sync"]);
            println!("{}✨ Done!{}", GREEN, NC);
        }
        "info" => do_info(),
        "clean" => do_clean(config.force_yes, config.force_no),
        "shell" => enter_activated_shell(),
        _ => {
            println!("{}❌ Unknown command: {}{}", RED, config.command, NC);
            print_help();
        }
    }
}

fn parse_args() -> Option<Config> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return None;
    }

    let mut config = Config {
        force_yes: args.iter().any(|a| a == "-y"),
        force_no: args.iter().any(|a| a == "-n"),
        command: String::new(),
        version: String::from(DEFAULT_VERSION),
    };

    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    if positional.len() >= 1 {
        config.command = positional[0].clone();
    }
    if positional.len() >= 2 {
        config.version = positional[1].clone();
    }
    Some(config)
}

fn ask_confirm(prompt: &str, force_yes: bool, force_no: bool) -> bool {
    if force_yes {
        return true;
    }
    if force_no {
        return false;
    }
    print!("{}{} [y/N]: {}", YELLOW, prompt, NC);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();
    input == "y" || input == "yes"
}
fn do_init(mut config: Config) {
    if config.version.is_empty() {
        config.version = DEFAULT_VERSION.to_string();
        println!(
            "{}ℹ️ No version specified, using default: {DEFAULT_VERSION}{}",
            BLUE, NC
        );
    }

    if Path::new("pyproject.toml").exists() {
        let msg = "Project already initialized. Overwrite everything?";
        if !ask_confirm(msg, config.force_yes, config.force_no) {
            println!("Aborted.");
            return;
        }
        let _ = fs::remove_file("pyproject.toml");
        let _ = fs::remove_file(".python-version");
        let _ = fs::remove_file("uv.lock");
    }

    println!(
        "{}🚀 Initializing with Python {}...{}",
        GREEN, config.version, NC
    );

    if run_uv(&["init", "--python", &config.version]) {
        run_uv(&["python", "pin", &config.version]);

        if Path::new("requirements.txt").exists() {
            println!(
                "{}📄 Found requirements.txt, adding dependencies...{}",
                BLUE, NC
            );
            run_uv(&["add", "-r", "requirements.txt"]);
        }

        run_uv(&["sync"]);
        enter_activated_shell();
    }
}

fn do_info() {
    println!("{}📊 Project Status:{}", BLUE, NC);
    if let Ok(ver) = fs::read_to_string(".python-version") {
        println!("  {}Python:{}   {}", GREEN, NC, ver.trim());
    }
    if Path::new(".venv").exists() {
        println!("  {}Venv:{}     Ready (.venv)", GREEN, NC);
    } else {
        println!("  {}Venv:{}     Not found", RED, NC);
    }
    if let Ok(content) = fs::read_to_string("pyproject.toml") {
        let deps_count = content
            .lines()
            .filter(|l| l.trim().starts_with('"'))
            .count();
        println!("  {}Deps:{}     {} packages listed", GREEN, NC, deps_count);
    }
}

fn do_clean(f_y: bool, f_n: bool) {
    if !ask_confirm("Wipe all dependencies and lock file?", f_y, f_n) {
        return;
    }
    println!("{}🧹 Cleaning dependencies...{}", YELLOW, NC);
    let _ = fs::remove_file("uv.lock");
    if let Ok(content) = fs::read_to_string("pyproject.toml") {
        let mut new_lines = Vec::new();
        let mut in_deps = false;
        for line in content.lines() {
            if line.trim().starts_with("dependencies = [") {
                new_lines.push("dependencies = []");
                in_deps = true;
                continue;
            }
            if in_deps {
                if line.trim().ends_with(']') {
                    in_deps = false;
                }
                continue;
            }
            new_lines.push(line);
        }
        let _ = fs::write("pyproject.toml", new_lines.join("\n")).ok();
    }
    run_uv(&["sync"]);
    println!("{}✨ Project dependencies wiped.{}", GREEN, NC);
}

fn enter_activated_shell() {
    println!("{}✨ Activating environment...{}", GREEN, NC);

    let current_path = env::var_os("PATH").unwrap_or_default();
    let venv_bin = if cfg!(windows) {
        PathBuf::from(".venv").join("Scripts")
    } else {
        PathBuf::from(".venv").join("bin")
    };

    let full_venv_path = fs::canonicalize(&venv_bin).unwrap_or(venv_bin);
    let mut paths = vec![full_venv_path];
    paths.extend(env::split_paths(&current_path));
    let new_path = env::join_paths(paths).expect("Failed to build PATH");

    if cfg!(windows) {
        let is_cmd = env::var("PROMPT").is_ok();
        if is_cmd {
            Command::new("cmd")
                .env("PATH", &new_path)
                .arg("/K")
                .arg(".venv\\Scripts\\activate.bat")
                .spawn()
                .expect("Failed to start CMD")
                .wait()
                .ok();
        } else {
            Command::new("powershell")
                .env("PATH", &new_path)
                .arg("-NoExit")
                .arg("-Command")
                .arg(". .\\.venv\\Scripts\\Activate.ps1")
                .spawn()
                .expect("Failed to start PowerShell")
                .wait()
                .ok();
        }
    } else {
        let shell = env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
        Command::new(&shell)
            .env("PATH", &new_path)
            .env("VIRTUAL_ENV", fs::canonicalize(".venv").unwrap_or_default())
            .spawn()
            .expect("Failed to start shell")
            .wait()
            .ok();
    };
}

fn run_uv(args: &[&str]) -> bool {
    let success = Command::new("uv")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !success {
        println!("{}❌ uv command failed{}", RED, NC);
    }
    success
}

fn update_toml(new_ver: &str) -> std::io::Result<()> {
    let content = fs::read_to_string("pyproject.toml")?;
    let mut new_content = String::new();
    for line in content.lines() {
        if line.contains("requires-python") {
            new_content.push_str(&format!("requires-python = \">={}\"\n", new_ver));
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }
    fs::write("pyproject.toml", new_content)
}

fn print_help() {
    println!("{BLUE}uvp — UV Project Manager (Rust){NC}");
    println!("\nUsage:");
    println!("  uvp init [ver] [-y]   - Init project (default ver: {DEFAULT_VERSION})");
    println!("  uvp update <ver>      - Update pyproject.toml version");
    println!("  uvp pin [ver]         - Pin local python version");
    println!("  uvp info              - Show project status");
    println!("  uvp clean [-y]        - Wipe all dependencies from project");
    println!("  uvp shell             - Enter activated shell");
}
