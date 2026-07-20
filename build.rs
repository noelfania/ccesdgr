use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;

fn main() {
    if !cfg!(target_os = "windows") {
        return;
    }

    println!("cargo:rerun-if-changed=assets/icon-entry-3.png");
    println!("cargo:rerun-if-changed=assets/draw-pen.png");
    println!("cargo:rerun-if-changed=assets/app.manifest");

    let src = image::open("assets/icon-entry-3.png")
        .expect("assets/icon-entry-3.png 읽기 실패")
        .into_rgba8();

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let icon_dir = out_dir.join("generated-icons");
    fs::create_dir_all(&icon_dir).expect("아이콘 출력 폴더 생성 실패");

    let app_ico = icon_dir.join("app.ico");
    // 파란 배경 PNG → 단일 멀티사이즈 ICO (exe·트레이 공용)
    write_ico_from_rgba(&app_ico, &src, &[16, 24, 32, 48, 256]);

    // 드로우 펜 커서 — 클래식 BMP CUR (폴백) + 런타임 틴트용 RGBA
    let pen_cur = out_dir.join("draw_pen.cur");
    write_pen_cursor_classic(&pen_cur);
    write_pen_rgba_bin(&out_dir.join("draw_pen_rgba.bin"));

    // 커스텀 RC: 아이콘·커서·ComCtl32 v6 매니페스트 (set_resource_file 시 set_manifest 무시됨)
    let manifest = env::current_dir()
        .expect("cwd")
        .join("assets/app.manifest");
    assert!(
        manifest.is_file(),
        "assets/app.manifest 없음: {}",
        manifest.display()
    );
    let rc_path = icon_dir.join("app.rc");
    write_resource_script(&rc_path, &app_ico, &pen_cur, &manifest);

    let mut resource = winresource::WindowsResource::new();
    resource.set_resource_file(&rc_path.to_string_lossy());
    resource.compile().expect("Windows 리소스 컴파일 실패");
}

/// Win32 CreateIconFromResourceEx / CURSOR 리소스용 클래식 .cur (DIB, PNG 아님)
fn write_pen_cursor_classic(path: &Path) {
    let rgba = image::open("assets/draw-pen.png")
        .expect("assets/draw-pen.png 읽기 실패")
        .into_rgba8();
    let (w, h) = rgba.dimensions();
    assert!(w == 32 && h == 32, "draw-pen.png 는 32x32 이어야 함");
    let (hot_x, hot_y) = pen_tip_hotspot(&rgba);

    // XOR: 32bpp BGRA bottom-up, AND: 1bpp padded row
    let xor_size = (w * h * 4) as usize;
    let and_row = ((w as usize + 31) / 32) * 4;
    let and_size = and_row * h as usize;
    let dib_header = 40u32;
    let image_size = dib_header as usize + xor_size + and_size;

    let mut dib = Vec::with_capacity(image_size);
    // BITMAPINFOHEADER (biHeight = h*2 for XOR+AND)
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&((h as i32) * 2).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bit count
    dib.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&(xor_size as u32).to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes()); // x ppm
    dib.extend_from_slice(&0u32.to_le_bytes()); // y ppm
    dib.extend_from_slice(&0u32.to_le_bytes()); // colors used
    dib.extend_from_slice(&0u32.to_le_bytes()); // important colors

    for y in (0..h).rev() {
        for x in 0..w {
            let p = rgba.get_pixel(x, y).0;
            dib.push(p[2]); // B
            dib.push(p[1]); // G
            dib.push(p[0]); // R
            dib.push(p[3]); // A
        }
    }
    dib.resize(dib_header as usize + xor_size + and_size, 0); // AND mask = 0 (alpha 사용)

    let mut cur = Vec::with_capacity(6 + 16 + image_size);
    cur.extend_from_slice(&0u16.to_le_bytes()); // reserved
    cur.extend_from_slice(&2u16.to_le_bytes()); // type = cursor
    cur.extend_from_slice(&1u16.to_le_bytes()); // count
    // ICONDIRENTRY — cursor: planes=hotspot_x, bitcount=hotspot_y
    cur.push(w as u8);
    cur.push(h as u8);
    cur.push(0); // color count
    cur.push(0); // reserved
    cur.extend_from_slice(&hot_x.to_le_bytes());
    cur.extend_from_slice(&hot_y.to_le_bytes());
    cur.extend_from_slice(&(image_size as u32).to_le_bytes());
    cur.extend_from_slice(&22u32.to_le_bytes()); // offset (6+16)
    cur.extend_from_slice(&dib);

    fs::write(path, &cur).expect("CUR 쓰기 실패");
}

/// [u32 w][u32 h][u32 hot_x][u32 hot_y][RGBA…] — 동적 커서 틴트용
fn write_pen_rgba_bin(path: &Path) {
    let rgba = image::open("assets/draw-pen.png")
        .expect("assets/draw-pen.png 읽기 실패")
        .into_rgba8();
    let (w, h) = rgba.dimensions();
    let (hot_x, hot_y) = pen_tip_hotspot(&rgba);
    let mut out = Vec::with_capacity(16 + rgba.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&(hot_x as u32).to_le_bytes());
    out.extend_from_slice(&(hot_y as u32).to_le_bytes());
    out.extend_from_slice(rgba.as_raw());
    fs::write(path, &out).expect("pen rgba bin 쓰기 실패");
}

fn pen_tip_hotspot(rgba: &image::RgbaImage) -> (u16, u16) {
    let (w, h) = rgba.dimensions();
    let mut best = i32::MAX;
    let mut hot = (2u16, 28u16);
    for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y)[3] < 200 {
                continue;
            }
            // 왼쪽·아래 우선 (펜촉)
            let score = (x as i32) * 3 + (h as i32 - 1 - y as i32);
            if score < best {
                best = score;
                hot = (x as u16, y as u16);
            }
        }
    }
    hot
}

fn write_ico_from_rgba(path: &Path, src: &image::RgbaImage, sizes: &[u32]) {
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in sizes {
        let resized = image::imageops::resize(src, size, size, FilterType::Lanczos3);
        let image = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&image).expect("ICO encode 실패"));
    }
    let mut file = fs::File::create(path).expect("ICO 파일 생성 실패");
    dir.write(&mut file).expect("ICO 쓰기 실패");
}

fn write_resource_script(rc_path: &Path, app: &Path, pen_cur: &Path, manifest: &Path) {
    let to_rc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
    // 101 = 드로우 펜 커서 / 1 24 = RT_MANIFEST (ComCtl32 v6 비주얼 스타일)
    let content = format!(
        "1 ICON \"{}\"\n101 CURSOR \"{}\"\n1 24 \"{}\"\n",
        to_rc(app),
        to_rc(pen_cur),
        to_rc(manifest)
    );
    let mut file = fs::File::create(rc_path).expect("RC 파일 생성 실패");
    file.write_all(content.as_bytes())
        .expect("RC 파일 쓰기 실패");
}
