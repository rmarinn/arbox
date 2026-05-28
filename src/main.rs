use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

use arbox::{image, launch};

#[derive(Parser, Debug)]
#[command(
    name = "arbox",
    version,
    about = "Docker-based agent sandbox: a skinny chroot of the host"
)]
struct Cli {
    /// Mount HOST_PATH read-write (repeatable, global).
    #[arg(long = "rw", value_name = "PATH", global = true)]
    rw: Vec<PathBuf>,

    /// Mount HOST_PATH read-only (repeatable, global).
    #[arg(long = "ro", value_name = "PATH", global = true)]
    ro: Vec<PathBuf>,

    /// Shortcut for --rw $HOME/Desktop (fails if missing).
    #[arg(long = "desktop", global = true)]
    desktop: bool,

    /// Shortcut for --rw $HOME/Downloads (fails if missing).
    #[arg(long = "downloads", global = true)]
    downloads: bool,

    /// Passes in 'ANTHROPIC_API_KEY' when using claude
    #[arg(long, global = true)]
    pass_ai_env_vars: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run Claude Code (--dangerously-skip-permissions injected).
    ///
    /// All trailing args are forwarded to claude:
    ///   `arbox claude --resume`, `arbox claude "describe this repo"`.
    Claude {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        /// Passes in 'ANTHROPIC_API_KEY'
        #[arg(long, global = true)]
        pass_ai_env_vars: bool,
    },
    /// Run Codex CLI (approval-bypass flag injected).
    ///
    /// Passes --dangerously-bypass-approvals-and-sandbox. All trailing args
    /// are forwarded to codex.
    Codex {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run Google Antigravity's `agy` CLI.
    ///
    /// The binary is baked into the image; ~/.gemini and
    /// ~/.config/antigravity mount from the host for credential / skill /
    /// MCP persistence. First-time auth uses agy's SSH-style URL+code flow
    /// because libsecret isn't available inside the container. All
    /// trailing args forwarded.
    Agy {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run xAI's Grok Build (`grok`) CLI.
    ///
    /// The binary is baked into the image; ~/.grok mounts from the host
    /// for auth (token in ~/.grok/auth.json) and download cache. All
    /// trailing args forwarded — grok's safety story is plan-mode review,
    /// not an approval-bypass flag.
    Grok {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Interactive bash login shell inside the sandbox.
    Bash,
    /// Run the Playwright CLI (test, codegen, show-report, …).
    ///
    /// Image ships node + playwright + chromium + firefox + the system
    /// libs they link against. Examples: `arbox playwright test`,
    /// `arbox playwright codegen https://example.com`,
    /// `arbox playwright show-report`.
    Playwright {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run an arbitrary command: `arbox run -- cargo test`.
    Run {
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
    /// Build (or rebuild) the sandbox image for this host.
    Build {
        /// Force a rebuild even if the image already exists.
        #[arg(long)]
        force: bool,
        /// Pass --no-cache to docker build.
        #[arg(long)]
        no_cache: bool,
    },
    /// Show host facts, image presence, and mount layout.
    Status,
    /// Remove every arbox image for this host.
    Clean,
}

fn main() -> ExitCode {
    if std::env::var_os("ARBOX_INSIDE").is_some() {
        eprintln!("arbox is the host-side orchestrator; it cannot run inside its own container.");
        eprintln!("Exit this shell and run `arbox` from your host.");
        return ExitCode::FAILURE;
    }
    let cli = Cli::parse();
    let mut rw = cli.rw;
    if cli.desktop || cli.downloads {
        let Some(home) = std::env::var_os("HOME") else {
            eprintln!("error: --desktop/--downloads require $HOME to be set");
            return ExitCode::FAILURE;
        };
        let home = PathBuf::from(home);
        if cli.desktop {
            rw.push(home.join("Desktop"));
        }
        if cli.downloads {
            rw.push(home.join("Downloads"));
        }
    }
    match dispatch(cli.cmd, rw, cli.ro) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(cmd: Cmd, rw: Vec<PathBuf>, ro: Vec<PathBuf>) -> Result<ExitCode> {
    match cmd {
        Cmd::Claude { args, pass_ai_env_vars } => launch::run_claude(args, rw, ro, pass_ai_env_vars),
        Cmd::Codex { args } => launch::run_codex(args, rw, ro),
        Cmd::Agy { args } => launch::run_agy(args, rw, ro),
        Cmd::Grok { args } => launch::run_grok(args, rw, ro),
        Cmd::Bash => launch::run_bash(rw, ro),
        Cmd::Playwright { args } => launch::run_playwright(args, rw, ro),
        Cmd::Run { cmd } => launch::run_argv(cmd, rw, ro, &[]),
        Cmd::Build { force, no_cache } => {
            image::build_image(force, no_cache).map(|_| ExitCode::SUCCESS)
        }
        Cmd::Status => image::print_status().map(|_| ExitCode::SUCCESS),
        Cmd::Clean => image::clean().map(|_| ExitCode::SUCCESS),
    }
}
