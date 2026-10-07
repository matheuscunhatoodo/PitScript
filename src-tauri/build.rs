fn main() {
    let mut attributes = tauri_build::Attributes::new();
    // Unit-test executables do not inherit Tauri's binary resource manifest.
    // Wry imports TaskDialogIndirect, which requires Common Controls v6.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Let the linker embed the same dependency for binaries and test harnesses.
        // Avoid embedding it a second time through Tauri's .rc resources.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
    tauri_build::try_build(attributes).expect("Failed to build Tauri resources")
}
