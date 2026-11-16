//! Legacy detach.bin codec, lock-serialized atomic state and bounded recovery.
//! No external Rust crate: test on Linux, Termux and Android.
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)] use std::os::unix::fs::PermissionsExt;
#[cfg(unix)] use std::os::fd::AsRawFd;

const MAX_FILE: u64 = 1024 * 1024;
const MAX_BACKUPS: usize = 5;

pub fn valid_package(name: &str) -> bool {
    !name.is_empty() && name.len() <= 127 && name.contains('.') &&
        name.split('.').all(|part| !part.is_empty() &&
            part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
}

pub fn decode(data: &[u8]) -> io::Result<Vec<String>> {
    if data.len() as u64 > MAX_FILE { return invalid("configuration too large"); }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < data.len() {
        let len = data[i] as usize;
        i += 1;
        if len == 0 || len % 2 == 0 || len > data.len() - i { return invalid("truncated or invalid binary entry"); }
        let entry = &data[i..i + len];
        i += len;
        if entry.iter().enumerate().any(|(index, byte)| index % 2 != 0 && *byte != 0) {
            return invalid("invalid UTF-16LE binary encoding");
        }
        let bytes: Vec<u8> = entry.iter().step_by(2).copied().collect();
        let name = String::from_utf8(bytes).map_err(|_| error("invalid UTF-8 package name"))?;
        if !valid_package(&name) { return invalid("invalid package name"); }
        if seen.insert(name.clone()) { result.push(name); }
    }
    Ok(result)
}

fn error(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.to_string()) }
fn invalid<T>(message: &str) -> io::Result<T> { Err(error(message)) }

pub fn encode(names: &[String]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for name in names {
        if !valid_package(name) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid package name")); }
        if !seen.insert(name.as_str()) { continue; }
        let length = name.len() * 2 - 1;
        let length = u8::try_from(length).map_err(|_| error("package identifier too long"))?;
        output.push(length);
        for (index, byte) in name.bytes().enumerate() {
            output.push(byte);
            if index + 1 != name.len() { output.push(0); }
        }
        if output.len() as u64 > MAX_FILE { return invalid("configuration too large"); }
    }
    Ok(output)
}

pub fn from_text(input: &str) -> io::Result<Vec<String>> {
    let names = input.lines().map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned).collect::<Vec<_>>();
    decode(&encode(&names)?)
}

pub fn import_file(path: &Path) -> io::Result<Vec<String>> {
    if fs::metadata(path)?.len() > MAX_FILE { return invalid("import file too large"); }
    let data = fs::read(path)?;
    if data.len() as u64 > MAX_FILE { return invalid("import file too large"); }
    let source = std::str::from_utf8(&data).map_err(|_| error("import not UTF-8"))?;
    from_text(source)
}

pub fn load(path: &Path) -> io::Result<Vec<String>> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() { return invalid("configuration symlink refused"); }
    }
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    if file.metadata()?.len() > MAX_FILE { return invalid("configuration too large"); }
    let mut data = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut data)?;
    decode(&data)
}

struct FileLock(File);
#[cfg(unix)] unsafe extern "C" { fn flock(fd: i32, operation: i32) -> i32; }
fn lock(path: &Path) -> io::Result<FileLock> {
    let dir = path.parent().ok_or_else(|| error("missing configuration directory"))?;
    fs::create_dir_all(dir)?;
    let lock_path = dir.join(".detach.lock");
    if let Ok(meta) = fs::symlink_metadata(&lock_path) {
        if meta.file_type().is_symlink() { return invalid("lockfile symlink refused"); }
    }
    let file = OpenOptions::new().create(true).read(true).write(true).open(lock_path)?;
    #[cfg(unix)] {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        if unsafe { flock(file.as_raw_fd(), 2) } != 0 { return Err(io::Error::last_os_error()); }
    }
    Ok(FileLock(file))
}
impl Drop for FileLock {
    fn drop(&mut self) {
        #[cfg(unix)] { unsafe { flock(self.0.as_raw_fd(), 8); } }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| error("missing parent directory"))?;
    fs::create_dir_all(dir)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() { return invalid("refusing to overwrite symlink"); }
    }
    let mut temporary = None;
    for number in 0..128 {
        let candidate = dir.join(format!(".zd-{}-{}.tmp", std::process::id(), number));
        match OpenOptions::new().create_new(true).write(true).open(&candidate) {
            Ok(file) => { temporary = Some((candidate, file)); break; }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    let (temp_path, mut file) = temporary.ok_or_else(|| error("temporary filename unavailable"))?;
    let result = (|| -> io::Result<()> {
        #[cfg(unix)] file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(temp_path); }
    result
}

fn backups_dir(path: &Path) -> io::Result<PathBuf> {
    let dir = path.parent().ok_or_else(|| error("configuration parent missing"))?.join("backups");
    if let Ok(meta) = fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() || !meta.file_type().is_dir() {
            return invalid("backup directory is not an ordinary directory");
        }
    }
    fs::create_dir_all(&dir)?;
    if fs::symlink_metadata(&dir)?.file_type().is_symlink() {
        return invalid("backup directory symlink refused");
    }
    #[cfg(unix)] fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

fn listed_backups(path: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = backups_dir(path)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("backup-") && name.ends_with(".bin") &&
            name[7..name.len()-4].bytes().all(|b| b.is_ascii_digit() || b == b'-')) { continue; }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_file() { files.push(entry.path()); }
    }
    files.sort_by(|a,b| b.file_name().cmp(&a.file_name()));
    Ok(files)
}

fn backup_unlocked(path: &Path, names: &[String]) -> io::Result<PathBuf> {
    let data = encode(names)?;
    let dir = backups_dir(path)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| error("invalid system clock"))?.as_micros();
    let mut file = None;
    for index in 0..128 {
        let name = format!("backup-{stamp:020}-{:010}-{index:03}.bin", std::process::id());
        let candidate = dir.join(name);
        if !candidate.exists() { file = Some(candidate); break; }
    }
    let filename = file.ok_or_else(|| error("backup filename unavailable"))?;
    // Backup publication is itself atomic; no destructive overwrite.
    atomic_write(&filename, &data)?;
    for old in listed_backups(path)?.into_iter().skip(MAX_BACKUPS) { fs::remove_file(old)?; }
    Ok(filename)
}

pub fn backups(path: &Path) -> io::Result<Vec<PathBuf>> {
    let _guard = lock(path)?;
    listed_backups(path)
}

pub fn backup(path: &Path) -> io::Result<PathBuf> {
    let _guard = lock(path)?;
    backup_unlocked(path, &load(path)?)
}

pub fn restore(path: &Path, index: usize) -> io::Result<()> {
    let _guard = lock(path)?;
    let backups = listed_backups(path)?;
    let selected = backups.get(index).ok_or_else(|| error("backup index not found"))?;
    let restored = load(selected)?; // never overwrite state using corrupted backup
    if let Ok(current) = load(path) { backup_unlocked(path, &current)?; }
    atomic_write(path, &encode(&restored)?)
}

pub fn save(path: &Path, names: &[String]) -> io::Result<()> {
    let bytes = encode(names)?;
    let _guard = lock(path)?;
    let old = load(path)?; // corrupt files require explicit user recovery
    if encode(&old)? == bytes { return Ok(()); }
    backup_unlocked(path, &old)?;
    atomic_write(path, &bytes)
}

pub fn update<F>(path: &Path, edit: F) -> io::Result<bool>
where F: FnOnce(&mut Vec<String>) -> io::Result<()> {
    let _guard = lock(path)?;
    let mut names = load(path)?;
    let previous = names.clone();
    edit(&mut names)?;
    let new_bytes = encode(&names)?;
    if encode(&previous)? == new_bytes { return Ok(false); }
    backup_unlocked(path, &previous)?;
    atomic_write(path, &new_bytes)?;
    Ok(true)
}

#[cfg(test)] mod tests {
    use super::*;
    fn temporary(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("zd-test-{}-{}-{}", std::process::id(), name,
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&path).unwrap(); path
    }
    #[test] fn legacy_roundtrip() {
        let names = vec!["com.google.android.youtube".to_string(), "org.example_app.demo".to_string()];
        assert_eq!(decode(&encode(&names).unwrap()).unwrap(), names);
    }
    #[test] fn truncated_corrupt_and_oversized_refused() {
        for bytes in [vec![3, b'a'],vec![2,b'a',0],vec![1,255],vec![0],vec![3,b'a',b'x',b'b']] {
            assert!(decode(&bytes).is_err());
        }
        assert!(decode(&vec![0; 1024 * 1024 + 1]).is_err());
    }
    #[test] fn invalid_name_and_duplicates() {
        for name in ["", "abc", "a..b", "com.bad;id", "x/../y", "com.bad name"] {
            assert!(encode(&[name.to_string()]).is_err());
        }
        assert_eq!(decode(&encode(&["com.a.app".into(),"com.a.app".into()]).unwrap()).unwrap(),
            vec!["com.a.app".to_string()]);
    }
    #[test] fn atomic_save_and_backups_and_restore() {
        let dir=temporary("backups"); let path=dir.join("detach.bin");
        save(&path,&["com.first.app".into()]).unwrap();
        save(&path,&["com.second.app".into()]).unwrap();
        assert_eq!(load(&path).unwrap(),vec!["com.second.app".to_string()]);
        assert_eq!(backups(&path).unwrap().len(),2);
        restore(&path,0).unwrap();
        assert_eq!(load(&path).unwrap(),vec!["com.first.app".to_string()]);
        let _=fs::remove_dir_all(dir);
    }
    #[test] fn corrupt_input_does_not_overwrite() {
        let dir=temporary("corrupt");let path=dir.join("detach.bin");
        fs::write(&path,b"corrupt").unwrap();
        assert!(save(&path,&["com.valid.app".into()]).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"corrupt");
        let _=fs::remove_dir_all(dir);
    }
    #[test] fn update_idempotent_and_history_bounded() {
        let dir=temporary("update");let path=dir.join("detach.bin");
        assert!(update(&path,|names| {names.push("com.a.app".into());Ok(())}).unwrap());
        assert!(!update(&path,|_|Ok(())).unwrap());
        for i in 0..8 {save(&path,&[format!("com.app{i}.item")]).unwrap();}
        assert!(backups(&path).unwrap().len()<=5);
        let _=fs::remove_dir_all(dir);
    }
    #[test] fn bounded_import_refuses_oversized_file() {
        let dir=temporary("import");let path=dir.join("import.txt");
        fs::write(&path,vec![b'x';1024*1024+1]).unwrap();
        assert!(import_file(&path).is_err());
        let _=fs::remove_dir_all(dir);
    }
    #[cfg(unix)] #[test] fn symlink_backup_directory_refused() {
        use std::os::unix::fs::symlink;
        let dir=temporary("backup-link"); let external=temporary("external");
        let path=dir.join("detach.bin");
        symlink(&external, dir.join("backups")).unwrap();
        assert!(save(&path,&["com.good.app".into()]).is_err());
        assert!(!path.exists());
        let _=fs::remove_dir_all(dir);let _=fs::remove_dir_all(external);
    }
    #[cfg(unix)] #[test] fn symlink_configuration_refused() {
        use std::os::unix::fs::symlink;
        let dir=temporary("symlink");let path=dir.join("detach.bin");
        let other=dir.join("innocent");fs::write(&other,b"data").unwrap();
        symlink(&other,&path).unwrap();
        assert!(save(&path,&["com.good.app".into()]).is_err());
        assert_eq!(fs::read(&other).unwrap(),b"data");
        let _=fs::remove_dir_all(dir);
    }
}
