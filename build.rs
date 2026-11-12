//! Embeds the application icon in the Windows executable. Resource 1 is the
//! icon Explorer shows for the file, and the one GPUI gives the window and
//! its taskbar button.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app-icon/kutilang.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon_with_id("assets/app-icon/kutilang.ico", "1");
        resource.compile().expect("cannot embed the icon");
    }
}
