//! "New project" templates. Each one creates the folder, starter files, the language environment
//! (Python virtual environment, Rust first build, npm packages) and notes that tell the AI how this
//! project is run, all with the portable tools from `tools.rs`.
use crate::{
    netenv::ChildEnv,
    tools::{self, Layout, Report, PYTHON_VERSION},
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub needs: &'static str,
}
pub const TEMPLATES: [Template; 6] = [
    Template { id: "python-tk", name: "Python window app (tkinter)", description: "A small window with a text box and a button. Its own Python environment is created inside the project.", needs: "python" },
    Template { id: "python", name: "Python program", description: "A console program that asks a question and answers. Uses its own Python environment.", needs: "python" },
    Template { id: "rust-gui", name: "Rust window app (egui)", description: "A Rust window with a text box and a button. The first build takes about a minute.", needs: "rust" },
    Template { id: "rust", name: "Rust console app", description: "A Rust program that runs in a console window.", needs: "rust" },
    Template { id: "web-convex", name: "Web app with a Convex database", description: "A Node.js project with the Convex CLI ready for npx convex dev and deploy.", needs: "node" },
    Template { id: "empty", name: "Empty folder", description: "Just a folder. Ask the AI to set it up.", needs: "" },
];
pub fn templates_json() -> Value {
    json!(TEMPLATES.iter().map(|t| json!({"id": t.id, "name": t.name, "description": t.description, "needs": t.needs})).collect::<Vec<_>>())
}

const RESERVED: [&str; 22] = ["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];

/// A safe Windows folder name from what the student typed.
pub fn folder_name(name: &str) -> Result<String, String> {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '-' })
        .collect();
    let clean = clean.trim_matches(|c| c == ' ' || c == '.' || c == '-').to_string();
    if clean.is_empty() || clean.chars().count() > 60 {
        return Err("Use a project name of 1–60 letters, numbers, spaces, - or _.".into());
    }
    if RESERVED.contains(&clean.to_lowercase().as_str()) {
        return Err("That name is reserved by Windows. Choose another.".into());
    }
    Ok(clean)
}

/// A package name for cargo and uv: lowercase letters, digits, - and _, starting with a letter.
pub fn package_name(folder: &str) -> String {
    let mut s: String = folder
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() || !s.chars().next().unwrap().is_ascii_alphabetic() {
        format!("app-{s}").trim_end_matches('-').to_string()
    } else {
        s
    }
}

const PY_TK: &str = r#""""A small window app made with tkinter.

Press Run in RustPortableHarness, or type:  uv run main.py
"""
import tkinter as tk
from tkinter import ttk


def main() -> None:
    root = tk.Tk()
    root.title("My Python App")
    root.geometry("360x200")

    name = tk.StringVar()
    message = tk.StringVar(value="Type your name and press the button.")

    frame = ttk.Frame(root, padding=16)
    frame.pack(fill="both", expand=True)
    ttk.Label(frame, text="Your name:").pack(anchor="w")
    ttk.Entry(frame, textvariable=name).pack(fill="x", pady=4)
    ttk.Button(
        frame,
        text="Say hello",
        command=lambda: message.set(f"Hello, {name.get() or 'there'}!"),
    ).pack(pady=8)
    ttk.Label(frame, textvariable=message).pack()

    root.mainloop()


if __name__ == "__main__":
    main()
"#;
const PY_CONSOLE: &str = r#""""A small console program.

Press Run in RustPortableHarness, or type:  uv run main.py
"""


def main() -> None:
    name = input("What is your name? ")
    print(f"Hello, {name or 'there'}!")


if __name__ == "__main__":
    main()
"#;
const RS_CONSOLE: &str = r#"// A small console program. Press Run in RustPortableHarness, or type: cargo run
use std::io::{self, Write};

fn main() {
    print!("What is your name? ");
    io::stdout().flush().unwrap();
    let mut name = String::new();
    io::stdin().read_line(&mut name).unwrap();
    let name = name.trim();
    println!("Hello, {}!", if name.is_empty() { "there" } else { name });
}
"#;
const RS_GUI: &str = r#"// A small window app made with egui. Press Run in RustPortableHarness, or type: cargo run
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([420.0, 260.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My Rust App",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

#[derive(Default)]
struct MyApp {
    name: String,
    clicks: u32,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hello from Rust!");
            ui.horizontal(|ui| {
                ui.label("Your name:");
                ui.text_edit_singleline(&mut self.name);
            });
            if ui.button("Click me").clicked() {
                self.clicks += 1;
            }
            let who = if self.name.is_empty() { "there" } else { self.name.as_str() };
            ui.label(format!("Hello {who}! You clicked {} times.", self.clicks));
        });
    }
}
"#;
/// Smaller, faster builds on school computers: no debug info, light optimisation (the build folder
/// of a window app drops from about 1.7 GB to under 400 MB).
const RS_PROFILE: &str = "\n# Smaller, faster builds on school computers.\n[profile.dev]\nopt-level = 1\ndebug = false\n\n[profile.dev.package.\"*\"]\nopt-level = 2\ndebug = false\n";

fn agents_text(template: &str) -> String {
    let how = match template {
        "python-tk" | "python" => format!("This is a Python {PYTHON_VERSION} project managed with uv. Its environment is `.venv` in this folder.\n- Run: `uv run main.py`\n- Add a package: `uv add <name>` (never `pip install --user`).\n- tkinter is available for window apps."),
        "rust-gui" | "rust" => "This is a Rust project (toolchain stable-x86_64-pc-windows-gnu, no Visual Studio).\n- Run: `cargo run`\n- Add a crate: `cargo add <name>`. Avoid crates that need a C/C++ compiler, CMake or Visual Studio.\n- Window apps use eframe/egui with `default-features = false, features = [\"default_fonts\", \"glow\"]`.".into(),
        "web-convex" => "This is a Node.js project using Convex.\n- `npx convex dev` syncs the functions in `convex/` to a development deployment (the first run signs in and is interactive, so the student runs it with the Run button).\n- `npx convex deploy` publishes to production. It runs without questions when `CONVEX_DEPLOY_KEY` is set in `.env.local`.\n- Never print or commit `.env.local`.".into(),
        _ => "Ask the student what they want to build, then set the project up with the portable tools.".into(),
    };
    format!("# Project notes for the AI\n\n{how}\n\nThis school computer cannot install software. Use only the portable tools on PATH and keep every file inside this folder.\n")
}

fn readme(name: &str, template: &str) -> String {
    let run = match template {
        "python-tk" | "python" => "uv run main.py",
        "rust-gui" | "rust" => "cargo run",
        "web-convex" => "npm run dev   (first time: sign in to Convex when asked)",
        _ => "",
    };
    let mut text = format!("# {name}\n\nCreated with RustPortableHarness.\n");
    if !run.is_empty() {
        text.push_str(&format!("\n## Run it\n\nPress **Run** in the app, or open **Terminal** and type:\n\n    {run}\n"));
    }
    if template == "web-convex" {
        text.push_str("\n## Deploy to Convex\n\n1. Press **Run** once. `npx convex dev` asks you to sign in and create a Convex project.\n2. Ask the AI to add functions in the `convex/` folder.\n3. To publish, open **Terminal** and type `npx convex deploy`.\n   With a deploy key from the Convex dashboard in `.env.local` (`CONVEX_DEPLOY_KEY=...`), the AI can deploy for you.\n");
    }
    text
}

fn write(dir: &Path, file: &str, text: &str) -> Result<(), String> {
    let path = dir.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| format!("Cannot write {file}: {e}"))
}

/// The program to run for a portable tool, falling back to one installed on the PC.
fn program(tool_path: PathBuf, exe: &str) -> PathBuf {
    if tool_path.is_file() {
        tool_path
    } else {
        PathBuf::from(exe)
    }
}

/// Creates the project. Returns the folder.
pub async fn create(l: &Layout, parent: &Path, name: &str, template: &str, git: bool, env: &ChildEnv, report: &Report) -> Result<PathBuf, String> {
    let t = TEMPLATES.iter().find(|t| t.id == template).ok_or("Unknown project type")?;
    if !t.needs.is_empty() && !tools::usable(l, t.needs) {
        return Err(format!("{} is not set up yet. Open Programming tools and get it first.", tools::TOOLS.iter().find(|x| x.id == t.needs).map(|x| x.name).unwrap_or(t.needs)));
    }
    if template == "rust-gui" && tools::status(l, "rust").0 != "ready" {
        return Err("Rust window apps need the full Rust tool. Open Programming tools and get Rust again.".into());
    }
    let folder = folder_name(name)?;
    let pkg = package_name(&folder);
    std::fs::create_dir_all(parent).map_err(|e| format!("Cannot use {}: {e}", parent.display()))?;
    let dir = parent.join(&folder);
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("A folder named \"{folder}\" already exists there. Choose another name."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create the project folder: {e}"))?;
    report(Some(5), "Creating files…", None);
    match template {
        "python-tk" | "python" => {
            let uv = program(l.tools().join("uv").join("uv.exe"), "uv.exe");
            tools::run_logged(&uv, &["init", "--app", "--no-package", "--vcs", "none", "--no-workspace", "--python", PYTHON_VERSION, "--name", &pkg, "."], &dir, env, report, "Setting up the Python project…").await?;
            write(&dir, "main.py", if template == "python-tk" { PY_TK } else { PY_CONSOLE })?;
            write(&dir, ".gitignore", ".venv/\n__pycache__/\n*.pyc\n")?;
            report(Some(50), "Creating the Python environment (.venv)…", None);
            tools::run_logged(&uv, &["sync"], &dir, env, report, "Creating the Python environment (.venv)…").await?;
        }
        "rust" | "rust-gui" => {
            let cargo = program(l.cargo_home().join("bin").join("cargo.exe"), "cargo.exe");
            tools::run_logged(&cargo, &["init", "--vcs", "none", "--name", &pkg, "."], &dir, env, report, "Setting up the Rust project…").await?;
            if template == "rust-gui" {
                write(&dir, "Cargo.toml", &format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\neframe = {{ version = \"0.37\", default-features = false, features = [\"default_fonts\", \"glow\"] }}\n{RS_PROFILE}"))?;
                write(&dir, "src/main.rs", RS_GUI)?;
            } else {
                let toml = std::fs::read_to_string(dir.join("Cargo.toml")).map_err(|e| e.to_string())?;
                write(&dir, "Cargo.toml", &format!("{}\n{RS_PROFILE}", toml.trim_end()))?;
                write(&dir, "src/main.rs", RS_CONSOLE)?;
            }
            write(&dir, ".gitignore", "/target\n")?;
            report(Some(30), "Building for the first time (window apps take about a minute)…", None);
            tools::run_logged(&cargo, &["build"], &dir, env, report, "Building for the first time (window apps take about a minute)…").await?;
        }
        "web-convex" => {
            let node = program(l.tools().join("node").join("node.exe"), "node.exe");
            let npm_cli = l.tools().join("node").join("node_modules/npm/bin/npm-cli.js");
            let npm = |args: Vec<&str>| {
                let mut all = vec![];
                if npm_cli.is_file() {
                    all.push(npm_cli.to_string_lossy().to_string());
                }
                all.extend(args.iter().map(|s| s.to_string()));
                all
            };
            let (prog, init) = if npm_cli.is_file() { (node.clone(), npm(vec!["init", "-y"])) } else { (PathBuf::from("npm.cmd"), vec!["init".into(), "-y".into()]) };
            let init: Vec<&str> = init.iter().map(|s| s.as_str()).collect();
            tools::run_logged(&prog, &init, &dir, env, report, "Setting up the Node.js project…").await?;
            report(Some(40), "Installing the Convex CLI (npm install convex)…", None);
            let install = if npm_cli.is_file() { npm(vec!["install", "convex"]) } else { vec!["install".into(), "convex".into()] };
            let install: Vec<&str> = install.iter().map(|s| s.as_str()).collect();
            tools::run_logged(&prog, &install, &dir, env, report, "Installing the Convex CLI (npm install convex)…").await?;
            let mut pkg_json: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            pkg_json["scripts"] = json!({"dev": "convex dev", "deploy": "convex deploy"});
            pkg_json["name"] = json!(pkg);
            write(&dir, "package.json", &serde_json::to_string_pretty(&pkg_json).map_err(|e| e.to_string())?)?;
            write(&dir, ".gitignore", "node_modules/\n.env.local\n.env*.local\n")?;
            std::fs::create_dir_all(dir.join("convex")).map_err(|e| e.to_string())?;
        }
        _ => {}
    }
    write(&dir, "README.md", &readme(&folder, template))?;
    let notes = agents_text(template);
    write(&dir, "AGENTS.md", &notes)?;
    write(&dir, "CLAUDE.md", &notes)?;
    if git {
        let git = if l.git_exe().is_file() { l.git_exe() } else { PathBuf::from("git.exe") };
        report(Some(95), "Starting a Git repository…", None);
        tools::run_logged(&git, &["init", "-b", "main"], &dir, env, report, "Starting a Git repository…").await?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(folder_name("  My Game! ").unwrap(), "My Game");
        assert!(folder_name("con").is_err());
        assert!(folder_name("...").is_err());
        assert_eq!(package_name("My Game 2"), "my-game-2");
        assert_eq!(package_name("2048"), "app-2048");
        assert_eq!(package_name("Trò chơi"), "tr-ch-i");
    }
    #[tokio::test]
    async fn empty_project_writes_notes_and_refuses_existing_folder() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path(), &t.path().join("data"));
        let report: Report = std::sync::Arc::new(|_, _, _| {});
        let dir = create(&l, &t.path().join("projects"), "Demo", "empty", false, &ChildEnv::default(), &report).await.unwrap();
        assert!(dir.join("AGENTS.md").is_file() && dir.join("README.md").is_file());
        assert!(create(&l, &t.path().join("projects"), "Demo", "empty", false, &ChildEnv::default(), &report).await.is_err());
        assert!(create(&l, &t.path().join("projects"), "Py", "python", false, &ChildEnv::default(), &report).await.unwrap_err().contains("not set up"));
    }
}
