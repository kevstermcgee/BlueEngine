use vesper3d::viewer::props::{scene, CATALOG};
fn main() -> vesper3d::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .ok_or("Supply a new export folder")?;
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir)?;
    for def in &CATALOG {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(&scene(def.kind))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("{}.json", def.id)))?;
        file.write_all(&bytes)?;
    }
    Ok(())
}
