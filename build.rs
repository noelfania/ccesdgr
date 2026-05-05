fn main() {
    if cfg!(target_os = "windows") {
        println!("cargo:rerun-if-changed=assets/app-icon.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/app-icon.ico");
        resource.compile().expect("Windows 리소스 컴파일 실패");
    }
}
