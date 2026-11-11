//! Loading cheatsheets. Format is 100% compatible with the original
//! cheatbox `data/*.json` files.

use include_dir::{include_dir, Dir};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

static BUILTIN: Dir = include_dir!("$CARGO_MANIFEST_DIR/sheets");

#[derive(Debug, Clone)]
pub struct Entry {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub logo: Vec<String>,
    pub title: Vec<String>,
    pub outer_width: usize,
    pub primary_color: String,
    pub categories: Vec<Category>,
}

pub enum Source {
    Builtin(&'static str),
    File(PathBuf),
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::Builtin(_) => "built-in".into(),
            Source::File(p) => p.display().to_string(),
        }
    }
}

/// Default user directory for extra / overriding sheets.
pub fn user_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CHEATBOX_DIR") {
        return Some(PathBuf::from(d));
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("cheatbox"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("cheatbox"))
    }
}

/// All available sheets by name. Files in user/extra dirs override built-ins.
pub fn discover(extra: &[PathBuf]) -> BTreeMap<String, Source> {
    let mut map = BTreeMap::new();
    for f in BUILTIN.files() {
        if f.path().extension().is_some_and(|e| e == "json") {
            let name = f.path().file_stem().unwrap().to_string_lossy().to_string();
            if let Some(s) = f.contents_utf8() {
                map.insert(name, Source::Builtin(s));
            }
        }
    }
    let dirs = user_dir().into_iter().chain(extra.iter().cloned());
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|e| e == "json") {
                let name = p.file_stem().unwrap().to_string_lossy().to_string();
                if name != "ascii" {
                    map.insert(name, Source::File(p));
                }
            }
        }
    }
    map
}

pub fn load(src: &Source) -> Result<Sheet, String> {
    let text = match src {
        Source::Builtin(s) => s.to_string(),
        Source::File(p) => {
            std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?
        }
    };
    parse(&text).map_err(|e| format!("{}: {e}", src.describe()))
}

fn ascii(v: &Value) -> Vec<String> {
    // [ { "ascii": [ "line", ... ] } ]
    v.as_array()
        .and_then(|a| a.first())
        .and_then(|o| o.get("ascii"))
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn num(v: Option<&Value>, def: usize) -> usize {
    match v {
        Some(Value::Number(n)) => n.as_u64().map(|x| x as usize).unwrap_or(def),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(def),
        _ => def,
    }
}

pub fn parse(text: &str) -> Result<Sheet, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let obj = root.as_object().ok_or("top level must be an object")?;
    let style = obj.get("STYLE");
    let mut sheet = Sheet {
        logo: obj.get("LOGO").map(ascii).unwrap_or_default(),
        title: obj.get("TITLE").map(ascii).unwrap_or_default(),
        outer_width: num(style.and_then(|s| s.get("outer_width")), 130),
        primary_color: style
            .and_then(|s| s.get("primary_color"))
            .and_then(|v| v.as_str())
            .unwrap_or("blue")
            .to_string(),
        categories: Vec::new(),
    };
    for (k, v) in obj {
        if matches!(k.as_str(), "LOGO" | "TITLE" | "STYLE") {
            continue;
        }
        let Some(arr) = v.as_array() else { continue };
        let entries = arr
            .iter()
            .filter_map(|e| {
                Some(Entry {
                    command: e.get("command")?.as_str()?.to_string(),
                    description: e
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect();
        let entries: Vec<Entry> = entries;
        if !entries.is_empty() {
            sheet.categories.push(Category {
                name: k.clone(),
                entries,
            });
        }
    }
    Ok(sheet)
}
