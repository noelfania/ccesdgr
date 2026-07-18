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

    let src = image::open("assets/icon-entry-3.png")
        .expect("assets/icon-entry-3.png 읽기 실패")
        .into_rgba8();

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let icon_dir = out_dir.join("generated-icons");
    fs::create_dir_all(&icon_dir).expect("아이콘 출력 폴더 생성 실패");

    let app_ico = icon_dir.join("app.ico");
    // 파란 배경 PNG → 단일 멀티사이즈 ICO (exe·트레이 공용)
    write_ico_from_rgba(&app_ico, &src, &[16, 24, 32, 48, 256]);

    let rc_path = icon_dir.join("app.rc");
    write_resource_script(&rc_path, &app_ico);

    let mut resource = winresource::WindowsResource::new();
    resource.set_resource_file(&rc_path.to_string_lossy());
    resource.compile().expect("Windows 리소스 컴파일 실패");
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

fn write_resource_script(rc_path: &Path, app: &Path) {
    let to_rc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
    let content = format!("1 ICON \"{}\"\n", to_rc(app));
    let mut file = fs::File::create(rc_path).expect("RC 파일 생성 실패");
    file.write_all(content.as_bytes())
        .expect("RC 파일 쓰기 실패");
}
