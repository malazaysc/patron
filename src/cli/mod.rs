pub enum Command {
    Serve,
    Init { init_git: bool },
    Doctor,
    Help,
    Version,
}

pub fn parse() -> Result<Command, String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => Ok(Command::Serve),
        Some("serve") => Ok(Command::Serve),
        Some("init") => parse_init(args),
        Some("doctor") => Ok(Command::Doctor),
        Some("--help") | Some("-h") | Some("help") => Ok(Command::Help),
        Some("--version") | Some("-V") | Some("version") => Ok(Command::Version),
        Some(other) => Err(format!("unknown Patron command `{other}`")),
    }
}

fn parse_init(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut init_git = false;
    for arg in args {
        match arg.as_str() {
            "--git" => init_git = true,
            other => return Err(format!("unknown Patron init option `{other}`")),
        }
    }
    Ok(Command::Init { init_git })
}

pub fn help_text() -> String {
    "\
Patron

USAGE:
  patron [command]

COMMANDS:
  serve      Start the Patron web app on http://127.0.0.1:3000
  init       Initialize /.patron/ in the current repository
  doctor     Inspect repository and runtime readiness without mutating state
  help       Show this help output
  version    Show the Patron version

DEFAULT:
  If no command is provided, Patron runs `serve`.

INSTALL:
  cargo install --path .

EXAMPLES:
  patron init
  patron init --git
  patron doctor
  patron serve
"
    .to_string()
}

pub fn version_text() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
