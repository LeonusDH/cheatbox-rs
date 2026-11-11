mod colors;
mod markup;
mod render;
mod sheet;

use clap::{Parser, Subcommand, ValueEnum};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "cheatbox", version, about = "Cheatsheets in a bento box (Rust port of fatinul/cheatbox)")]
struct Cli {
    /// Extra directory with *.json sheets (can repeat). Also read: $CHEATBOX_DIR or the user dir (see `cheatbox path`)
    #[arg(long = "data-dir", global = true)]
    data_dir: Vec<PathBuf>,
    /// Output width in columns (default: sheet's outer_width, capped to the terminal)
    #[arg(short, long, global = true)]
    width: Option<usize>,
    /// When to use colors
    #[arg(long, value_enum, default_value_t = ColorMode::Auto, global = true)]
    color: ColorMode,
    /// Hide the ASCII logo/title header
    #[arg(long, global = true)]
    no_header: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand)]
enum Cmd {
    /// List available sheets
    List,
    /// Show a sheet by name
    Show {
        name: String,
        /// Only these categories (case-insensitive substring, can repeat)
        #[arg(short, long)]
        only: Vec<String>,
    },
    /// Random sheet with N random boxes — made for MOTD / login banners
    #[command(alias = "motd")]
    Random {
        /// Pick from these sheets only (comma separated)
        #[arg(short, long, value_delimiter = ',')]
        sheets: Vec<String>,
        /// How many random boxes to show (0 = all)
        #[arg(short, long, default_value_t = 2)]
        boxes: usize,
    },
    /// One random command as a single line ("tip of the day")
    Tip {
        #[arg(short, long, value_delimiter = ',')]
        sheets: Vec<String>,
    },
    /// Print the user directory for custom sheets
    Path,
}

fn main() -> ExitCode {
    #[cfg(windows)]
    let _ = enable_ansi_support::enable_ansi_support();

    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cheatbox: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let tty = std::io::stdout().is_terminal();
    let color = match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => tty && std::env::var_os("NO_COLOR").is_none(),
    };
    let sheets = sheet::discover(&cli.data_dir);
    let out = Output { width: cli.width, header: !cli.no_header, color, tty };

    match cli.cmd {
        Some(Cmd::List) => {
            for (name, src) in &sheets {
                println!("{name:<16} {}", src.describe());
            }
            Ok(())
        }
        Some(Cmd::Path) => {
            match sheet::user_dir() {
                Some(d) => println!("{}", d.display()),
                None => return Err("cannot determine user directory".into()),
            }
            Ok(())
        }
        Some(Cmd::Show { name, only }) => {
            let src = sheets.get(&name).ok_or_else(|| unknown(&name, &sheets))?;
            let mut s = sheet::load(src)?;
            if !only.is_empty() {
                let only: Vec<String> = only.iter().map(|o| o.to_lowercase()).collect();
                s.categories.retain(|c| only.iter().any(|o| c.name.to_lowercase().contains(o)));
                if s.categories.is_empty() {
                    return Err("no categories matched --only".into());
                }
            }
            out.print(&s)
        }
        Some(Cmd::Random { sheets: filter, boxes }) => {
            let (_, mut s) = pick_random(&sheets, &filter)?;
            if boxes > 0 && boxes < s.categories.len() {
                fastrand::shuffle(&mut s.categories);
                s.categories.truncate(boxes);
            }
            out.print(&s)
        }
        Some(Cmd::Tip { sheets: filter }) => {
            let (name, s) = pick_random(&sheets, &filter)?;
            let pairs: Vec<_> = s.categories.iter().flat_map(|c| c.entries.iter().map(move |e| (c, e))).collect();
            let (c, e) = pairs.get(fastrand::usize(..pairs.len().max(1))).ok_or("sheet is empty")?;
            println!("{}", markup::to_string(&render::tip(&name, &s, c, e), color));
            Ok(())
        }
        None => {
            if !tty || !std::io::stdin().is_terminal() {
                return Err("not a terminal: use `cheatbox show <name>` or `cheatbox random`".into());
            }
            let names: Vec<&String> = sheets.keys().collect();
            let idx = dialoguer::FuzzySelect::new()
                .with_prompt("CheatBox> Select the cheatsheet to display")
                .items(&names)
                .default(0)
                .interact_opt()
                .map_err(|e| e.to_string())?;
            let Some(i) = idx else { return Ok(()) };
            out.print(&sheet::load(&sheets[names[i]])?)
        }
    }
}

fn unknown(name: &str, sheets: &std::collections::BTreeMap<String, sheet::Source>) -> String {
    let all: Vec<&str> = sheets.keys().map(|s| s.as_str()).collect();
    format!("unknown sheet '{name}'. Available: {}", all.join(", "))
}

fn pick_random(
    sheets: &std::collections::BTreeMap<String, sheet::Source>,
    filter: &[String],
) -> Result<(String, sheet::Sheet), String> {
    let pool: Vec<&String> = if filter.is_empty() {
        sheets.keys().collect()
    } else {
        for f in filter {
            if !sheets.contains_key(f) {
                return Err(unknown(f, sheets));
            }
        }
        filter.iter().collect()
    };
    if pool.is_empty() {
        return Err("no sheets available".into());
    }
    let name = pool[fastrand::usize(..pool.len())].clone();
    let s = sheet::load(&sheets[&name])?;
    Ok((name, s))
}

struct Output {
    width: Option<usize>,
    header: bool,
    color: bool,
    tty: bool,
}

impl Output {
    fn print(&self, s: &sheet::Sheet) -> Result<(), String> {
        let term = if self.tty {
            terminal_size::terminal_size().map(|(w, _)| w.0 as usize)
        } else {
            None
        };
        let width = self.width.unwrap_or_else(|| term.map_or(s.outer_width, |t| t.min(s.outer_width)));
        let lines = render::render(s, &render::Opts { width, header: self.header });
        let mut so = std::io::stdout().lock();
        for l in lines {
            // ignore broken pipe (e.g. `| head`)
            if writeln!(so, "{}", markup::to_string(&l, self.color)).is_err() {
                break;
            }
        }
        Ok(())
    }
}
