//! File-manager clipboard, undo/redo and filesystem helpers for navigation mode.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipOp {
    Copy,
    Cut,
}

#[derive(Clone, Debug)]
pub struct FmClipboard {
    pub items: Vec<PathBuf>,
    pub op: ClipOp,
}

pub type Clipboard = Arc<Mutex<Option<FmClipboard>>>;

#[derive(Debug, Clone)]
pub enum UndoAction {
    DeletedFiles {
        items: Vec<(PathBuf, PathBuf)>,
    },
    CreatedFile {
        path: PathBuf,
    },
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    Paste {
        op: ClipOp,
        items: Vec<(PathBuf, PathBuf)>,
        saved_clipboard: Option<FmClipboard>,
        saved_yank_paths: std::collections::HashSet<String>,
        saved_cut_paths: std::collections::HashSet<String>,
        previous_dir: Option<PathBuf>,
        target_dir: Option<PathBuf>,
    },
}

pub type UndoStack = Arc<Mutex<Vec<UndoAction>>>;

fn get_deletion_date() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let sec_in_day = secs % 86400;

    let hour = sec_in_day / 3600;
    let min = (sec_in_day % 3600) / 60;
    let sec = sec_in_day % 60;

    let mut year = 1970;
    let mut days_left = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if days_left < days_in_year {
            break;
        }
        days_left -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &d in &month_days {
        if days_left < d {
            break;
        }
        days_left -= d;
        month += 1;
    }
    let day = days_left + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hour, min, sec
    )
}

fn percent_encode(s: &str) -> String {
    let mut res = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                res.push(b as char);
            }
            _ => {
                res.push_str(&format!("%{:02X}", b));
            }
        }
    }
    res
}

pub fn move_to_trash(path: &Path) -> std::io::Result<PathBuf> {
    let trash_dir = dirs::data_local_dir()
        .map(|d| d.join("Trash"))
        .unwrap_or_else(|| std::env::temp_dir().join("mm_trash"));

    let files_dir = trash_dir.join("files");
    let info_dir = trash_dir.join("info");

    fs::create_dir_all(&files_dir)?;
    fs::create_dir_all(&info_dir)?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_string());

    let mut backup = files_dir.join(&name);
    let mut info_file = info_dir.join(format!("{}.trashinfo", name));
    if backup.exists() {
        let mut n = 1u32;
        loop {
            let candidate_name = format!("{}_{}", n, name);
            let candidate_backup = files_dir.join(&candidate_name);
            let candidate_info = info_dir.join(format!("{}.trashinfo", candidate_name));
            if !candidate_backup.exists() {
                backup = candidate_backup;
                info_file = candidate_info;
                break;
            }
            n += 1;
        }
    }

    let absolute_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    move_path(path, &backup)?;

    let path_str = absolute_path.to_string_lossy();
    let encoded_path = percent_encode(&path_str);
    let date_str = get_deletion_date();
    let info_content = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        encoded_path, date_str
    );

    fs::write(info_file, info_content)?;

    Ok(backup)
}

pub fn copy_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        copy_dir_all(src, dst)
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

pub fn apply_undo(action: &UndoAction) -> std::io::Result<()> {
    match action {
        UndoAction::DeletedFiles { items } => {
            for (original, backup) in items {
                let _ = move_path(backup, original);
                if let Some(name) = backup.file_name()
                    && let Some(parent) = backup.parent()
                    && let Some(grandparent) = parent.parent()
                {
                    let info_file = grandparent
                        .join("info")
                        .join(format!("{}.trashinfo", name.to_string_lossy()));
                    let _ = fs::remove_file(info_file);
                }
            }
            Ok(())
        }
        UndoAction::CreatedFile { path } => {
            if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            }
        }
        UndoAction::Renamed { from, to } => move_path(to, from),
        UndoAction::Paste { op, items, .. } => {
            for (_src, dest) in items {
                match op {
                    ClipOp::Copy => {
                        if dest.is_dir() {
                            let _ = fs::remove_dir_all(dest);
                        } else {
                            let _ = fs::remove_file(dest);
                        }
                    }
                    ClipOp::Cut => {
                        let _ = move_path(dest, _src);
                    }
                }
            }
            Ok(())
        }
    }
}

pub fn apply_redo(action: &UndoAction) -> std::io::Result<()> {
    match action {
        UndoAction::DeletedFiles { items } => {
            for (original, _) in items {
                let _ = move_to_trash(original);
            }
            Ok(())
        }
        UndoAction::CreatedFile { path } => {
            if path.to_string_lossy().ends_with('/') {
                fs::create_dir_all(path)
            } else {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    let _ = fs::create_dir_all(parent);
                }
                fs::File::create(path).map(|_| ())
            }
        }
        UndoAction::Renamed { from, to } => move_path(from, to),
        UndoAction::Paste { op, items, .. } => {
            for (src, dest) in items {
                match op {
                    ClipOp::Copy => {
                        copy_path(src, dest)?;
                    }
                    ClipOp::Cut => {
                        move_path(src, dest)?;
                    }
                }
            }
            Ok(())
        }
    }
}

pub fn archive_stem(name: &str) -> &str {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    for suffix in &[".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar.lz4"] {
        if let Some(s) = base.strip_suffix(suffix) {
            return s;
        }
    }
    Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(base)
}

pub fn extract_archive(src: &str, dest_dir: &str) -> std::io::Result<()> {
    let lower = src.to_ascii_lowercase();
    let status = if lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tar.lz4")
        || lower.ends_with(".tar")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tbz2")
    {
        Command::new("tar")
            .args(["-xf", src, "-C", dest_dir])
            .status()
    } else if lower.ends_with(".zip") {
        Command::new("unzip")
            .args(["-q", src, "-d", dest_dir])
            .status()
    } else if lower.ends_with(".7z") {
        Command::new("7z")
            .args(["x", src, &format!("-o{dest_dir}")])
            .status()
    } else if lower.ends_with(".rar") {
        Command::new("unrar")
            .args(["x", src, &format!("{dest_dir}/")])
            .status()
    } else {
        return Err(std::io::Error::other(format!(
            "unsupported archive format: {src}"
        )));
    }?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "extractor exited with status {status}"
        )))
    }
}

pub fn create_archive(archive_name: &str, paths: &[String]) -> std::io::Result<()> {
    use std::process::Command;
    let lower = archive_name.to_ascii_lowercase();
    let is_tar = lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tar");

    let final_name = if is_tar || lower.ends_with(".zip") || lower.ends_with(".7z") {
        archive_name.to_string()
    } else {
        format!("{}.zip", archive_name)
    };

    let lower_final = final_name.to_ascii_lowercase();
    let status = if lower_final.ends_with(".tar.gz") || lower_final.ends_with(".tgz") {
        Command::new("tar")
            .arg("-czf")
            .arg(&final_name)
            .args(paths)
            .status()
    } else if lower_final.ends_with(".tar.bz2") || lower_final.ends_with(".tbz2") {
        Command::new("tar")
            .arg("-cjf")
            .arg(&final_name)
            .args(paths)
            .status()
    } else if lower_final.ends_with(".tar.xz") {
        Command::new("tar")
            .arg("-cJf")
            .arg(&final_name)
            .args(paths)
            .status()
    } else if lower_final.ends_with(".tar") {
        Command::new("tar")
            .arg("-cf")
            .arg(&final_name)
            .args(paths)
            .status()
    } else if lower_final.ends_with(".zip") {
        Command::new("zip")
            .arg("-r")
            .arg(&final_name)
            .args(paths)
            .status()
    } else if lower_final.ends_with(".7z") {
        Command::new("7z")
            .arg("a")
            .arg(&final_name)
            .args(paths)
            .status()
    } else {
        return Err(std::io::Error::other(format!(
            "unsupported archive creation format: {final_name}"
        )));
    }?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "archiver exited with status {status}"
        )))
    }
}

pub fn copy_into(src: &Path, dest_dir: &Path) -> std::io::Result<PathBuf> {
    let file_name = src
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;

    let dest = unique_dest(dest_dir, Path::new(file_name));

    if src.is_dir() {
        copy_dir_all(src, &dest)?;
    } else {
        fs::copy(src, &dest)?;
    }
    Ok(dest)
}

pub fn move_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    match fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(_) => {
            if src.is_dir() {
                copy_dir_all(src, dst)?;
                fs::remove_dir_all(src)
            } else {
                fs::copy(src, dst)?;
                fs::remove_file(src)
            }
        }
    }
}

pub fn move_into(src: &Path, dest_dir: &Path) -> std::io::Result<PathBuf> {
    let file_name = src
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;

    let dest = unique_dest(dest_dir, Path::new(file_name));
    move_path(src, &dest)?;
    Ok(dest)
}

fn unique_dest(dir: &Path, name: &Path) -> PathBuf {
    let stem = name.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = name
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    let base = dir.join(name);
    if !base.exists() {
        return base;
    }
    let mut n = 1u32;
    loop {
        let candidate = dir.join(format!("{stem}_{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}
