fn main() {
    println!("cargo:rerun-if-changed=assets/numlon.ico");
    println!("cargo:rerun-if-changed=assets/numlon-paused.ico");
    println!("cargo:rerun-if-changed=src/numlon.exe.manifest");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_manifest(include_str!("src/numlon.exe.manifest"));
    resource.set("FileDescription", "Numlon");
    resource.set("ProductName", "Numlon");
    resource.set("OriginalFilename", "numlon.exe");

    if std::path::Path::new("assets/numlon.ico").exists() {
        resource.set_icon("assets/numlon.ico");
    }
    if std::path::Path::new("assets/numlon-paused.ico").exists() {
        resource.set_icon_with_id("assets/numlon-paused.ico", "2");
    }

    if let Err(error) = resource.compile() {
        panic!("failed to compile Windows resources: {error}");
    }
}
