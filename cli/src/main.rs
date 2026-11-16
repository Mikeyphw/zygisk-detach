use std::error::Error;
use std::fmt::{Debug, Display};
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::panic::Location;
use std::process::{Command, ExitCode};

use termion::event::Key;
use termion::{clear, cursor, terminal_size};

mod colorize;
use colorize::ToColored;

mod menus;
use menus::Menus;
mod config;
mod doctor;
use std::path::Path;

#[cfg(target_os = "android")]
const MODULE_DETACH: &str = "/data/adb/zygisk-detach/detach.bin";
#[cfg(target_os = "android")]
const DETACH_TXT: &str = "/data/adb/modules/zygisk-detach/detach.txt";

#[cfg(target_os = "linux")]
const MODULE_DETACH: &str = "detach.bin";
#[cfg(target_os = "linux")]
const DETACH_TXT: &str = "detach.txt";

struct LocErr<E: Error> {
    source: E,
    loc: &'static Location<'static>,
}
impl<E: Error> Error for LocErr<E> {}
impl<E: Error> Debug for LocErr<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\r\nat {}", self.source, self.loc)
    }
}
impl<E: Error> Display for LocErr<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
impl From<io::Error> for LocErr<io::Error> {
    #[track_caller]
    fn from(err: io::Error) -> Self {
        Self {
            source: err,
            loc: Location::caller(),
        }
    }
}
type IOResult<T> = Result<T, LocErr<io::Error>>;

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|panic| {
        let mut stderr = io::stderr();
        let _ = writeln!(stderr, "\r\n{panic}\r\n");
        let _ = writeln!(stderr, "This should not have happened.");
        let _ = writeln!(
            stderr,
            "Report at https://github.com/j-hc/zygisk-detach/issues"
        );
    }));

    // Read-only CLI commands never create device state; config::lock creates it for mutations.

    let mut args = std::env::args().skip(1);
    if let Some(cmd) = args.next().as_deref() {
        match cmd {
            "serialize" => {
                let Some(dtxt) = args.next() else {
                    eprintln!("ERROR: detach.txt path not supplied.");
                    return ExitCode::FAILURE;
                };
                let Some(dbin) = args.next() else {
                    eprintln!("ERROR: detach.bin path not supplied.");
                    return ExitCode::FAILURE;
                };
                if let Err(err) = serialize_txt(&dtxt, &dbin) {
                    eprintln!("ERROR: {err}");
                    return ExitCode::FAILURE;
                }
                println!("Serialized detach.txt");
                return ExitCode::SUCCESS;
            }
            "detachall" => {
                let names: Vec<String> = args.collect();
                if names.is_empty() {
                    eprintln!("ERROR: No Package name(s) was supplied.");
                    return ExitCode::FAILURE;
                }
                if let Err(err) = config::save(Path::new(MODULE_DETACH), &names) {
                    eprintln!("ERROR: {err}");
                    return ExitCode::FAILURE;
                }
                detach_bin_changed();
                println!("Configuration saved. Runtime hook still requires verification.");
                return ExitCode::SUCCESS;
            }

            "detach" => {
                if args.len() == 0 {
                    eprintln!("ERROR: No Package name(s) was supplied.");
                    return ExitCode::FAILURE;
                }
                let requested: Vec<String> = args.collect();
                if requested.is_empty() { eprintln!("ERROR: No package names"); return ExitCode::FAILURE; }
                let result = config::update(Path::new(MODULE_DETACH), |names| {
                    for name in &requested {
                        if !config::valid_package(name) {
                            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid package name"));
                        }
                        if !names.contains(name) { names.push(name.clone()); }
                    }
                    Ok(())
                });
                match result {
                    Ok(changed) => {
                        if changed { detach_bin_changed(); }
                        println!("Configuration {}. Hook remains unverified.", if changed {"saved"} else {"unchanged"});
                    },
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "reset" => {
                if let Err(err) = config::save(Path::new(MODULE_DETACH), &[]) {
                    eprintln!("ERROR: {err}");
                    return ExitCode::FAILURE;
                }
                detach_bin_changed();
                println!("Configuration cleared.");
                return ExitCode::SUCCESS;
            }

            "reattach" => {
                let Some(pkg_name) = args.next() else {
                    eprintln!("ERROR: package name not supplied.");
                    return ExitCode::FAILURE;
                };
                match reattach_by_name(&pkg_name) {
                    Ok(true) => println!("re-attached: {pkg_name}"),
                    Ok(false) => println!("not detached: {pkg_name}"),
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "list" => {
                match config::load(Path::new(MODULE_DETACH)) {
                    Ok(names) => for name in names { println!("{name}"); },
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "verify" => {
                if args.next().is_some() {eprintln!("ERROR: verify takes no arguments");return ExitCode::FAILURE;}
                let names = match config::load(Path::new(MODULE_DETACH)) {
                    Ok(names) => names,
                    Err(e) => {eprintln!("ERROR: {e}");return ExitCode::FAILURE;}
                };
                let result = doctor::collect(names.len());
                doctor::print(&result, false);
                if result.hook_state == "observed_in_current_store_process" {return ExitCode::SUCCESS;}
                eprintln!("HOOK_NOT_VERIFIED: configuration is not proof of detachment");
                return ExitCode::FAILURE;
            }
            "status" | "doctor" => {
                let names = match config::load(Path::new(MODULE_DETACH)) {
                    Ok(names) => names,
                    Err(e) => {eprintln!("ERROR: {e}");return ExitCode::FAILURE;}
                };
                let mut json = false;
                let mut bundle: Option<String> = None;
                while let Some(arg) = args.next() {
                    match arg.as_str() {
                        "--json" => json=true,
                        "--bundle" if bundle.is_none() => {
                            let Some(path) = args.next() else {eprintln!("ERROR: --bundle requires file path");return ExitCode::FAILURE;};
                            bundle=Some(path);
                        },
                        _ => {eprintln!("ERROR: unexpected option {arg}");return ExitCode::FAILURE;}
                    }
                }
                let report=doctor::collect(names.len());
                if let Some(file) = bundle {
                    if let Err(e)=doctor::write_bundle(&report,Path::new(&file)) {
                        eprintln!("ERROR: diagnostic export refused: {e}");return ExitCode::FAILURE;
                    }
                }
                doctor::print(&report,json);
                return ExitCode::SUCCESS;
            }

            "backups" => {
                match config::backups(Path::new(MODULE_DETACH)) {
                    Ok(files) => for (i, file) in files.iter().enumerate() {
                        println!("{i}: {}",file.file_name().unwrap_or_default().to_string_lossy());
                    },
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "backup" => {
                match config::backup(Path::new(MODULE_DETACH)) {
                    Ok(file) => println!("Backup saved: {}",file.display()),
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "restore" => {
                let Some(value) = args.next() else { eprintln!("ERROR: restore requires backup index"); return ExitCode::FAILURE; };
                let index = match value.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => {eprintln!("ERROR: invalid backup index");return ExitCode::FAILURE;}
                };
                if args.next().is_some() {eprintln!("ERROR: too many arguments");return ExitCode::FAILURE;}
                match config::restore(Path::new(MODULE_DETACH), index) {
                    Ok(()) => {detach_bin_changed(); println!("Backup restored; live hook unverified");},
                    Err(e) => { eprintln!("ERROR: {e}");return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "export" => {
                match config::load(Path::new(MODULE_DETACH)) {
                    Ok(names) => for name in names { println!("{name}"); },
                    Err(e) => { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                }
                return ExitCode::SUCCESS;
            }
            "import" => {
                let Some(path) = args.next() else { eprintln!("ERROR: import requires a file path"); return ExitCode::FAILURE; };
                let result = config::import_file(Path::new(&path))
                    .and_then(|names| config::save(Path::new(MODULE_DETACH), &names));
                if let Err(e) = result { eprintln!("ERROR: {e}"); return ExitCode::FAILURE; }
                detach_bin_changed();
                println!("Configuration imported; runtime hook remains unverified.");
                return ExitCode::SUCCESS;
            }

            s => {
                eprintln!(
                    "\
Unexpected command: {s}
Accepted commands are: 'detach', 'detachall', 'reattach', 'reset', 'list', 'serialize', 'status', 'doctor', 'verify', 'export', 'import', 'backups', 'backup', 'restore'"
                );
                return ExitCode::FAILURE;
            }
        }
    }

    #[cfg(target_os = "android")]
    let _ = Command::new("magisk")
        .args(["--denylist", "rm", "com.android.vending"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut p| p.wait());

    let mut menus = Menus::new();
    let ret = match interactive(&mut menus) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\rERROR: {err}");
            ExitCode::FAILURE
        }
    };
    menus.cursor_show().unwrap();
    ret
}

fn detach_bin_changed() {
    let _ = fs::remove_file(DETACH_TXT);
    let _ = kill_store();
}

fn serialize_txt(txt: &str, bin: &str) -> IOResult<()> {
    let contents = fs::read_to_string(txt)?;
    let names = config::from_text(&contents)?;
    config::save(Path::new(bin), &names)?;
    Ok(())
}

fn interactive(menus: &mut Menus) -> IOResult<()> {
    menus.cursor_hide()?;
    print!("zygisk-detach cli by github.com/j-hc\r\n\n");
    loop {
        match main_menu(menus)? {
            Op::DetachSelect => detach_menu(menus)?,
            Op::ReattachSelect => reattach_menu(menus)?,
            Op::Reset => {
                if config::save(Path::new(MODULE_DETACH), &[]).is_ok() {
                    detach_bin_changed();
                    text!(menus, "Reset");
                } else {
                    return Err(io::Error::new(io::ErrorKind::Other, "could not reset configuration").into());
                }
            }
            Op::CopyToSd => {
                #[cfg(target_os = "android")]
                const SDCARD_DETACH: &str = "/sdcard/detach.bin";
                #[cfg(target_os = "linux")]
                const SDCARD_DETACH: &str = "detach_sdcard.bin";
                match fs::copy(MODULE_DETACH, SDCARD_DETACH) {
                    Ok(_) => text!(menus, "Copied"),
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        text!(menus, "detach.bin not found");
                    }
                    Err(err) => return Err(err.into()),
                }
            }
            Op::Quit => return Ok(()),
            Op::Nop => {}
        }
    }
}

fn reattach_menu(menus: &mut Menus) -> IOResult<()> {
    let names = config::load(Path::new(MODULE_DETACH))?;
    if names.is_empty() {
        text!(menus, "No detached packages");
        return Ok(());
    }
    let Some(i) = menus.select_menu(
        names.iter().map(String::as_str),
        "Select the app to re-attach ('q' to leave):",
        "✖".red(),
        Some(Key::Char('q')),
    )? else { return Ok(()); };
    if reattach_by_name(&names[i])? {
        textln!(menus, "{}: {}", "re-attach".red(), names[i]);
    }
    Ok(())
}

fn reattach_by_name(pkg_name: &str) -> IOResult<bool> {
    if !config::valid_package(pkg_name) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid package name").into());
    }
    let changed = config::update(Path::new(MODULE_DETACH), |names| {
        names.retain(|name| name != pkg_name); Ok(())
    })?;
    if changed { detach_bin_changed(); }
    Ok(changed)
}

#[cfg(target_os = "linux")]
fn get_installed_apps() -> IOResult<Vec<u8>> {
    Ok("package:com.app1\npackage:org.xxx2\ncom.apppppppp.tooolonnggggtooolonnggggtooolonnggggtooolonngggg".as_bytes().to_vec())
}

#[cfg(target_os = "android")]
fn get_installed_apps() -> IOResult<Vec<u8>> {
    let op = Command::new("pm")
        .args(["list", "packages"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;
    if !op.status.success() {
        panic!("pm: '{}'", String::from_utf8_lossy(&op.stderr));
    }
    Ok(op.stdout)
}

#[derive(Clone, Copy)]
enum Op {
    DetachSelect,
    ReattachSelect,
    Reset,
    CopyToSd,
    Quit,
    Nop,
}

fn main_menu(menus: &mut Menus) -> IOResult<Op> {
    struct OpText {
        desc: &'static str,
        op: Op,
    }
    impl OpText {
        fn new(desc: &'static str, op: Op) -> Self {
            Self { desc, op }
        }
    }
    impl Display for OpText {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.desc)
        }
    }
    let ops = [
        OpText::new("Detach", Op::DetachSelect),
        OpText::new("Re-attach", Op::ReattachSelect),
        OpText::new("Reset detached apps", Op::Reset),
        OpText::new("Copy detach.bin to /sdcard", Op::CopyToSd),
    ];
    let i = menus.select_menu_numbered(ops.iter(), Key::Char('q'), "- Selection:")?;
    use menus::SelectNumberedResp as SN;
    match i {
        SN::Index(i) => Ok(ops[i].op),
        SN::UndefinedKey(Key::Char(c)) => {
            text!(menus, "Undefined key {c:?}");
            Ok(Op::Nop)
        }
        SN::UndefinedKey(k @ (Key::Down | Key::Up | Key::Left | Key::Right)) => {
            text!(menus, "Undefined key {k:?}");
            Ok(Op::Nop)
        }
        SN::Quit => Ok(Op::Quit),
        _ => Ok(Op::Nop),
    }
}



fn detach_menu(menus: &mut Menus) -> IOResult<()> {
    let installed_apps = get_installed_apps()?;
    assert_ne!(installed_apps.len(), 0);
    let apps: Vec<&str> = installed_apps[..installed_apps.len() - 1]
        .split(|&e| e == b'\n')
        .map(|e| {
            e.get("package:".len()..)
                .expect("unexpected output from pm")
        })
        .map(|e| std::str::from_utf8(e).expect("non utf-8 package names?"))
        .collect();
    menus.cursor_show()?;
    // Preserve full package identifier even on a narrow terminal.
    let selected = menus.select_menu_with_input(
        |input| {
            let input = input.trim();
            if !input.is_empty() {
                apps.iter()
                    .filter(|app| {
                        app.to_ascii_lowercase()
                            .contains(&input.to_ascii_lowercase())
                    })
                    .take(5)
                    .copied()
                    .collect()
            } else {
                Vec::new()
            }
        },
        "↪".green(),
        "- app: ",
        None,
    )?;
    menus.cursor_hide()?;
    if let Some(detach_app) = selected {
        if detach_by_name(detach_app)? {
            textln!(menus, "{} {}", "detach:".green(), detach_app);
            textln!(menus, "Changes are applied. No need for a reboot!");
        } else {
            textln!(menus, "{} {}", "already detached:".green(), detach_app);
        }
    }
    Ok(())
}

fn detach_by_name(detach_app: &str) -> IOResult<bool> {
    if !config::valid_package(detach_app) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid package name").into());
    }
    let changed = config::update(Path::new(MODULE_DETACH), |names| {
        if !names.iter().any(|name| name == detach_app) { names.push(detach_app.into()); }
        Ok(())
    })?;
    if changed { detach_bin_changed(); }
    Ok(changed)
}

fn _kill_store_am() -> IOResult<()> {
    Command::new("am")
        .args(["force-stop", "com.android.vending"])
        .spawn()?
        .wait()?;
    Ok(())
}

fn kill_store() -> IOResult<()> {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    const PKG: &[u8] = b"com.android.vending";
    let mut buf = [0u8; PKG.len()];
    for proc in fs::read_dir("/proc")? {
        let mut proc = proc?.path();
        if !proc.is_dir() {
            continue;
        }
        proc.push("cmdline");
        let Ok(mut cmdline) = fs::OpenOptions::new().read(true).open(&proc) else {
            continue;
        };
        if !cmdline.read(&mut buf).is_ok_and(|n| n > 0) {
            continue;
        }
        if buf.eq(PKG)
            && let Some(pid) = proc.components().nth(2)
        {
            let pid = pid.as_os_str().to_string_lossy();
            let Ok(pid) = pid.parse::<i32>() else {
                continue;
            };
            unsafe { kill(pid, 9) };
        }
    }
    Ok(())
}
