//! Bundles the example's stylesheet.

fn main() -> Result<(), exos_build::Error> {
    exos_build::Assets::new().css("css/app.css")?.emit()
}
