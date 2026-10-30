//! The verbs. Plain file operations on a folder: no git, no state, no in-place edits.

use crate::config::{CONFIG, Config};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc};
use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

/// Make `dir` a substrate. Never overwrites: an existing substrate is an error, an existing AGENTS.md is kept.
pub fn init(dir: &Path, timezone: &str) -> Result<()> {
    if dir.join(CONFIG).exists() {
        bail!("already a substrate: {}", dir.display());
    }
    fs::create_dir_all(dir.join(".substrate"))?;
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    create_new(&dir.join(CONFIG), &include_str!("default/config.toml").replace("{timezone}", timezone))?;
    create_new(&dir.join("AGENTS.md"), &include_str!("default/AGENTS.md").replace("{name}", &name))?;
    Ok(())
}

/// Current time in the substrate's timezone, whatever the machine is set to.
pub fn now(cfg: &Config) -> Result<DateTime<FixedOffset>> {
    Ok(match &cfg.timezone {
        Some(name) => {
            let tz: chrono_tz::Tz = name.parse().map_err(|e| anyhow!("timezone {name:?}: {e}"))?;
            Utc::now().with_timezone(&tz).fixed_offset()
        }
        None => Local::now().fixed_offset(),
    })
}

/// The nearest substrate at or above `start`.
pub fn find(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|p| p.join(CONFIG).is_file()).map(Path::to_path_buf)
}

/// New file in the inbox. Never overwrites: a name clash gets a `-2`, `-3`... suffix.
pub fn inbox_add<Tz: TimeZone>(root: &Path, cfg: &Config, text: &str, now: DateTime<Tz>) -> Result<PathBuf>
where
    Tz::Offset: Display,
{
    let inbox = cfg.inbox.as_ref().context("this substrate has no [inbox] in its config")?;
    let dir = safe_join(root, &inbox.dir)?;
    fs::create_dir_all(&dir)?;
    let name = now.format(&inbox.filename).to_string();
    create_unique(&dir.join(name), &format!("{}\n", text.trim_end()))
}

/// Today's scratch file. Created from the template if missing, left alone if it exists.
pub fn scratch_today<Tz: TimeZone>(root: &Path, cfg: &Config, now: DateTime<Tz>) -> Result<PathBuf>
where
    Tz::Offset: Display,
{
    let scratch = cfg.scratch.as_ref().context("this substrate has no [scratch] in its config")?;
    let dir = safe_join(root, &scratch.dir)?;
    let path = dir.join(now.format(&scratch.filename).to_string());
    if path.exists() {
        return Ok(path);
    }
    let template = match &scratch.template {
        Some(t) => fs::read_to_string(safe_join(&root.join(".substrate"), t)?)?,
        None => "# {date}\n".into(),
    };
    let content = template.replace("{date}", &now.format("%Y-%m-%d").to_string());
    fs::create_dir_all(&dir)?;
    create_new(&path, &content)?;
    Ok(path)
}

/// Entries in a folder, sorted, dirs suffixed with `/`. Hidden entries (`.git`, `.substrate`) are skipped.
pub fn list(root: &Path, rel: &str) -> Result<Vec<String>> {
    entries(&safe_join(root, rel)?)
}

/// Like `list`, but recursive, drawn like `tree`.
pub fn tree(root: &Path, rel: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    tree_into(&safe_join(root, rel)?, "", &mut out)?;
    Ok(out)
}

pub fn read(root: &Path, rel: &str) -> Result<String> {
    Ok(fs::read_to_string(safe_join(root, rel)?)?)
}

/// Rename a file or folder. Into `to` if it's a folder, or ends with `/` (created if missing). Never overwrites.
pub fn move_path(root: &Path, from: &str, to_rel: &str) -> Result<PathBuf> {
    let from = safe_join(root, from)?;
    let mut to = safe_join(root, to_rel)?;
    if to_rel.ends_with('/') {
        fs::create_dir_all(&to)?;
    }
    if from == root {
        bail!("can't move the substrate itself");
    }
    if !from.exists() {
        bail!("no such file: {}", from.display());
    }
    if to.is_dir() {
        to.push(from.file_name().context("nothing to move")?);
    }
    if to.exists() {
        bail!("already exists: {}", to.display());
    }
    fs::rename(&from, &to)?;
    Ok(to)
}

/// Join a relative path onto root, refusing anything that could escape it. `..` is fine while it stays inside.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => parts.push(p),
            Component::CurDir => {}
            Component::ParentDir if parts.pop().is_some() => {}
            _ => bail!("path must stay inside the substrate: {rel}"),
        }
    }
    Ok(parts.iter().fold(root.to_path_buf(), |p, part| p.join(part)))
}

fn entries(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        out.push(if entry.file_type()?.is_dir() { name + "/" } else { name });
    }
    out.sort();
    Ok(out)
}

fn tree_into(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    let names = entries(dir)?;
    for (i, name) in names.iter().enumerate() {
        let last = i + 1 == names.len();
        out.push(format!("{prefix}{}{name}", if last { "└── " } else { "├── " }));
        if let Some(sub) = name.strip_suffix('/') {
            tree_into(&dir.join(sub), &format!("{prefix}{}", if last { "    " } else { "│   " }), out)?;
        }
    }
    Ok(())
}

/// Write `content` to `path` unless it exists. Returns whether it was written.
fn create_new(path: &Path, content: &str) -> Result<bool> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            f.write_all(content.as_bytes())?;
            Ok(true)
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn create_unique(path: &Path, content: &str) -> Result<PathBuf> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1.. {
        let candidate = if n == 1 {
            path.to_path_buf()
        } else {
            path.with_file_name(format!("{stem}-{n}{ext}"))
        };
        if create_new(&candidate, content)? {
            return Ok(candidate);
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 30, 14, 5, 9).unwrap()
    }

    #[test]
    fn inbox_add_creates_file_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::default();
        let a = inbox_add(tmp.path(), &cfg, "buy milk", now()).unwrap();
        let b = inbox_add(tmp.path(), &cfg, "buy eggs", now()).unwrap();
        assert_eq!(a, tmp.path().join("inbox/2026-10-30T140509.md"));
        assert_eq!(b, tmp.path().join("inbox/2026-10-30T140509-2.md"));
        assert_eq!(fs::read_to_string(a).unwrap(), "buy milk\n");
        assert_eq!(fs::read_to_string(b).unwrap(), "buy eggs\n");
    }

    #[test]
    fn scratch_today_creates_once_and_keeps_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::default();
        let p = scratch_today(tmp.path(), &cfg, now()).unwrap();
        assert_eq!(p, tmp.path().join("scratch/2026-10-30.md"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "# 2026-10-30\n");
        fs::write(&p, "my scratch").unwrap();
        scratch_today(tmp.path(), &cfg, now()).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "my scratch");
    }

    #[test]
    fn scratch_today_uses_template_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".substrate/templates")).unwrap();
        fs::write(tmp.path().join(".substrate/templates/scratch.md"), "# {date}\n\n50% done\n").unwrap();
        fs::write(
            tmp.path().join(".substrate/config.toml"),
            "[scratch]\ntemplate = \"templates/scratch.md\"\n",
        )
        .unwrap();
        let cfg = Config::load(tmp.path()).unwrap();
        let p = scratch_today(tmp.path(), &cfg, now()).unwrap();
        assert_eq!(fs::read_to_string(p).unwrap(), "# 2026-10-30\n\n50% done\n");
    }

    #[test]
    fn list_sorts_marks_dirs_and_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("ideas")).unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join("now.md"), "").unwrap();
        assert_eq!(list(tmp.path(), "").unwrap(), vec!["ideas/", "now.md"]);
    }

    #[test]
    fn paths_cannot_escape_root() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path(), "../etc/passwd").is_err());
        assert!(read(tmp.path(), "/etc/passwd").is_err());
        assert!(list(tmp.path(), "a/../..").is_err());
    }

    #[test]
    fn init_makes_a_substrate_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("writing");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("AGENTS.md"), "mine").unwrap();
        init(&dir, "Europe/Stockholm").unwrap();
        let cfg = Config::load(&dir).unwrap();
        assert_eq!(cfg.timezone.as_deref(), Some("Europe/Stockholm"));
        assert!(cfg.inbox.is_some() && cfg.scratch.is_some());
        assert_eq!(fs::read_to_string(dir.join("AGENTS.md")).unwrap(), "mine");
        assert!(init(&dir, "UTC").is_err());
    }

    #[test]
    fn init_writes_agents_md_named_after_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("renatum");
        fs::create_dir(&dir).unwrap();
        init(&dir, "UTC").unwrap();
        assert!(fs::read_to_string(dir.join("AGENTS.md")).unwrap().starts_with("# renatum\n"));
    }

    #[test]
    fn find_returns_the_nearest_substrate() {
        let tmp = tempfile::tempdir().unwrap();
        let writing = tmp.path().join("writing");
        let characters = writing.join("characters");
        fs::create_dir_all(&characters).unwrap();
        init(tmp.path(), "UTC").unwrap();
        assert_eq!(find(&characters).unwrap(), tmp.path());
        init(&writing, "UTC").unwrap();
        assert_eq!(find(&characters).unwrap(), writing);
    }

    #[test]
    fn missing_section_turns_the_feature_off() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".substrate")).unwrap();
        fs::write(tmp.path().join(CONFIG), "[inbox]\n").unwrap();
        let cfg = Config::load(tmp.path()).unwrap();
        assert!(inbox_add(tmp.path(), &cfg, "x", now()).is_ok());
        assert!(scratch_today(tmp.path(), &cfg, now()).is_err());
    }

    #[test]
    fn now_uses_the_configured_timezone() {
        let cfg = Config { timezone: Some("Asia/Tokyo".into()), ..Config::default() };
        assert_eq!(super::now(&cfg).unwrap().offset().local_minus_utc(), 9 * 3600);
        let bad = Config { timezone: Some("Mars/Olympus".into()), ..Config::default() };
        assert!(super::now(&bad).is_err());
    }

    #[test]
    fn parent_dirs_are_fine_while_inside() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("a")).unwrap();
        fs::write(tmp.path().join("now.md"), "hi").unwrap();
        assert_eq!(read(tmp.path(), "a/../now.md").unwrap(), "hi");
    }

    #[test]
    fn move_into_folder_and_never_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("inbox")).unwrap();
        fs::create_dir_all(tmp.path().join("ideas")).unwrap();
        fs::write(tmp.path().join("inbox/x.md"), "x").unwrap();
        fs::write(tmp.path().join("inbox/y.md"), "y").unwrap();
        fs::write(tmp.path().join("ideas/y.md"), "old").unwrap();
        assert_eq!(move_path(tmp.path(), "inbox/x.md", "ideas").unwrap(), tmp.path().join("ideas/x.md"));
        assert_eq!(move_path(tmp.path(), "ideas/x.md", "ideas/z.md").unwrap(), tmp.path().join("ideas/z.md"));
        assert!(move_path(tmp.path(), "inbox/y.md", "ideas").is_err());
        assert_eq!(fs::read_to_string(tmp.path().join("ideas/y.md")).unwrap(), "old");
        assert!(move_path(tmp.path(), "inbox/y.md", "../y.md").is_err());
    }

    #[test]
    fn move_with_trailing_slash_creates_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("x.md"), "x").unwrap();
        assert_eq!(move_path(tmp.path(), "x.md", "inbox/").unwrap(), tmp.path().join("inbox/x.md"));
    }

    #[test]
    fn tree_draws_nested_folders_and_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("writing/characters")).unwrap();
        fs::create_dir(tmp.path().join(".substrate")).unwrap();
        fs::write(tmp.path().join("writing/characters/ana.md"), "").unwrap();
        fs::write(tmp.path().join("writing/about.md"), "").unwrap();
        fs::write(tmp.path().join("now.md"), "").unwrap();
        assert_eq!(
            tree(tmp.path(), "").unwrap(),
            vec![
                "├── now.md",
                "└── writing/",
                "    ├── about.md",
                "    └── characters/",
                "        └── ana.md",
            ]
        );
    }
}
