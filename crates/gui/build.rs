//! Build script: embeds Windows executable resources.
//!
//! - `assets/res/icon2.ico` becomes the app icon (shown by Explorer / the taskbar).
//! - `assets/res/manifest.xml` is the application manifest; its
//!   `requestedExecutionLevel=requireAdministrator` gives the exe the UAC shield
//!   and makes it elevate on launch (the kernel driver needs admin rights).

fn main() {
    #[cfg(windows)]
    {
        // Rebuild if the embedded resources change.
        println!("cargo:rerun-if-changed=assets/res/icon2.ico");
        //println!("cargo:rerun-if-changed=assets/res/manifest.xml");

        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/res/icon2.ico");
        //res.set_manifest_file("assets/res/manifest.xml");
        if let Err(e) = res.compile() {
            panic!("failed to embed Windows resources: {e}");
        }
    }
}
