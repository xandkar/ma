use std::{
    fs,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use flate2::{read::GzDecoder, write::GzEncoder};

pub fn read_gz<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<u8>> {
    let contents = fs::read(path)?;
    let mut decoder = GzDecoder::new(&contents[..]);
    let mut buff = Vec::new();
    decoder.read_to_end(&mut buff)?;
    Ok(buff)
}

/// XXX Appends ".gz" to the given path.
pub fn write_as_gz<P: AsRef<Path>, D: AsRef<[u8]>>(
    path: P,
    data: D,
) -> anyhow::Result<()> {
    let path = path_extension_append(path, "gz");
    if let Some(parent) = &path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(&path)?;
    let gz_encoder =
        GzEncoder::new(BufWriter::new(file), flate2::Compression::default());
    let mut writer = BufWriter::new(gz_encoder);
    writer.write_all(data.as_ref())?;
    Ok(())
}

// Because I want to be able to rename:
//     foo.txt --> foo.txt.gz
// instead of just:
//     foo.txt --> foo.gz
// that we get with path.with_extension.
fn path_extension_append<P: AsRef<Path>>(path: P, ext: &str) -> PathBuf {
    let path = path.as_ref();
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = match path.extension() {
        None => {
            format!("{}.{}", stem, ext)
        }
        Some(ext0) => {
            format!("{}.{}.{}", stem, ext0.to_string_lossy(), ext)
        }
    };
    path.with_file_name(name)
}
