//! The verbs. Plain file operations on a folder: no git, no state, no in-place edits.

use crate::config::Config;
use anyhow::{Result, bail};
use chrono::{DateTime, Local};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

/// New file in the inbox. Never overwrites: a name clash gets a `-2`, `-3`... suffix.
pub fn inbox_add(root: &Path, cfg: &Config, text: &str, now: DateTime<Local>) -> Result<PathBuf> {
    let dir = safe_join(root, &cfg.inbox.dir)?;
    fs::create_dir_all(&dir)?;
    let name = now.format(&cfg.inbox.filename).to_string();
    create_unique(&dir.join(name), &format!("{}\n", text.trim_end()))
}

/// Today's scratch file. Created from the template if missing, left alone if it exists.
pub fn daily_today(root: &Path, cfg: &Config, now: DateTime<Local>) -> Result<PathBuf> {
    let dir = safe_join(root, &cfg.daily.dir)?;
    let path = dir.join(now.format(&cfg.daily.filename).to_string());
    if path.exists() {
        return Ok(path);
    }
    let template = match &cfg.daily.template {
        Some(t) => fs::read_to_string(safe_join(&root.join(".substrate"), t)?)?,
        None => "# {date}\n".into(),
    };
    let content = template.replace("{date}", &now.format("%Y-%m-%d").to_string());
    fs::create_dir_all(&dir)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => f.write_all(content.as_bytes())?,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    Ok(path)
}

/// Entries in a folder, sorted, dirs suffixed with `/`. Hidden entries (`.git`, `.substrate`) are skipped.
pub fn list(root: &Path, rel: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(safe_join(root, rel)?)? {
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

pub fn read(root: &Path, rel: &str) -> Result<String> {
    Ok(fs::read_to_string(safe_join(root, rel)?)?)
}

/// Join a relative path onto root, refusing anything that could escape it.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = Path::new(rel);
    for c in rel.components() {
        if !matches!(c, Component::Normal(_) | Component::CurDir) {
            bail!("path must stay inside the root: {}", rel.display());
        }
    }
    Ok(root.join(rel))
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
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(mut f) => {
                f.write_all(content.as_bytes())?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
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
    fn daily_today_creates_once_and_keeps_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::default();
        let p = daily_today(tmp.path(), &cfg, now()).unwrap();
        assert_eq!(p, tmp.path().join("daily/2026-10-30.md"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "# 2026-10-30\n");
        fs::write(&p, "my scratch").unwrap();
        daily_today(tmp.path(), &cfg, now()).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "my scratch");
    }

    #[test]
    fn daily_today_uses_template_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".substrate/templates")).unwrap();
        fs::write(tmp.path().join(".substrate/templates/daily.md"), "# {date}\n\n50% done\n").unwrap();
        fs::write(
            tmp.path().join(".substrate/config.toml"),
            "[daily]\ntemplate = \"templates/daily.md\"\n",
        )
        .unwrap();
        let cfg = Config::load(tmp.path()).unwrap();
        let p = daily_today(tmp.path(), &cfg, now()).unwrap();
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
}
