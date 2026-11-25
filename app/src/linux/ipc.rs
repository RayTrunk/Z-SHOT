// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Commands between the Z-SHOT processes over Unix sockets in the user's runtime
//! directory (`$XDG_RUNTIME_DIR/z-shot`, mode 0700): `daemon.sock` for the background
//! process, `ui.sock` for the window process that is open right now.
//!
//! One connection carries one command line, e.g. `region` or `clipboard <path>`.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Region,
    Full,
    Show,
    Settings,
    Quit,
    /// Register the global shortcuts again (opens the desktop's shortcut dialog).
    Shortcuts,
    /// `settings.json` changed (update check on/off).
    Reload,
    /// Put this PNG (in the runtime directory) on the clipboard and delete it.
    Clipboard(PathBuf),
}

impl Command {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        let (word, arg) = line.split_once(' ').unwrap_or((line, ""));
        Some(match word {
            "region" => Self::Region,
            "full" => Self::Full,
            "show" => Self::Show,
            "settings" => Self::Settings,
            "quit" => Self::Quit,
            "shortcuts" => Self::Shortcuts,
            "reload" => Self::Reload,
            "clipboard" if !arg.is_empty() => Self::Clipboard(PathBuf::from(arg)),
            _ => return None,
        })
    }

    pub fn to_line(&self) -> String {
        match self {
            Self::Region => "region".into(),
            Self::Full => "full".into(),
            Self::Show => "show".into(),
            Self::Settings => "settings".into(),
            Self::Quit => "quit".into(),
            Self::Shortcuts => "shortcuts".into(),
            Self::Reload => "reload".into(),
            Self::Clipboard(p) => format!("clipboard {}", p.display()),
        }
    }

    /// The argument of `--ui` / `--capture` for this command, if it opens a window.
    pub fn ui_arg(&self) -> Option<&'static str> {
        match self {
            Self::Region => Some("region"),
            Self::Full => Some("full"),
            Self::Show => Some("show"),
            Self::Settings => Some("settings"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Socket {
    Daemon,
    Ui,
}

/// `$XDG_RUNTIME_DIR/z-shot`, created with mode 0700. Without `XDG_RUNTIME_DIR`
/// (rare: no systemd/elogind session) a per-user folder in the temp directory.
pub fn runtime_dir() -> PathBuf {
    let dir = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(base) if !base.is_empty() => PathBuf::from(base).join("z-shot"),
        _ => {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            std::env::temp_dir().join(format!("z-shot-{user}"))
        }
    };
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    dir
}

fn socket_path(socket: Socket) -> PathBuf {
    runtime_dir().join(match socket {
        Socket::Daemon => "daemon.sock",
        Socket::Ui => "ui.sock",
    })
}

/// Sends `cmd`; `false` if no process listens on that socket.
pub fn send(socket: Socket, cmd: &Command) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path(socket)) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    stream.write_all(format!("{}\n", cmd.to_line()).as_bytes()).is_ok()
}

pub fn is_running(socket: Socket) -> bool {
    UnixStream::connect(socket_path(socket)).is_ok()
}

pub fn remove_socket(socket: Socket) {
    let _ = std::fs::remove_file(socket_path(socket));
}

pub enum ListenError {
    /// Another process already listens on this socket.
    AlreadyRunning,
    Io(std::io::Error),
}

/// Binds the socket and hands every received command to `handler` on a listener
/// thread. A socket file left behind by a crashed process is replaced.
pub fn listen(socket: Socket, handler: impl Fn(Command) + Send + 'static) -> Result<(), ListenError> {
    let path = socket_path(socket);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&path).is_ok() {
                return Err(ListenError::AlreadyRunning);
            }
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path).map_err(ListenError::Io)?
        }
        Err(e) => return Err(ListenError::Io(e)),
    };
    std::thread::Builder::new()
        .name(format!("ipc-{socket:?}"))
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Some(cmd) = read_command(stream) {
                    handler(cmd);
                }
            }
        })
        .map_err(ListenError::Io)?;
    Ok(())
}

fn read_command(stream: UnixStream) -> Option<Command> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    BufReader::new(stream.take(4096)).read_line(&mut line).ok()?;
    // An empty connection is a liveness check (`is_running`), not a command.
    if line.trim().is_empty() {
        return None;
    }
    let cmd = Command::parse(&line);
    if cmd.is_none() {
        crate::log::log(format!("Unbekannter Befehl verworfen: {:?}", line.trim_end()));
    }
    cmd
}

/// Whether `path` lies directly in the runtime directory (files handed over by
/// another Z-SHOT process; anything else is refused).
pub fn is_runtime_file(path: &Path) -> bool {
    let dir = runtime_dir();
    match (path.parent().and_then(|p| p.canonicalize().ok()), dir.canonicalize()) {
        (Some(parent), Ok(dir)) => parent == dir,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_roundtrip() {
        for cmd in [
            Command::Region,
            Command::Full,
            Command::Show,
            Command::Settings,
            Command::Quit,
            Command::Shortcuts,
            Command::Reload,
            Command::Clipboard(PathBuf::from("/run/user/1000/z-shot/clip 1.png")),
        ] {
            assert_eq!(Command::parse(&format!("{}\n", cmd.to_line())), Some(cmd));
        }
        assert_eq!(Command::parse("rm -rf /"), None);
        assert_eq!(Command::parse("clipboard"), None);
    }

    #[test]
    fn only_files_in_the_runtime_dir_are_accepted() {
        let inside = runtime_dir().join("clip-test.png");
        std::fs::write(&inside, b"x").unwrap();
        assert!(is_runtime_file(&inside));
        let _ = std::fs::remove_file(&inside);
        assert!(!is_runtime_file(Path::new("/etc/passwd")));
        assert!(!is_runtime_file(&runtime_dir().join("..").join("passwd")));
    }
}
