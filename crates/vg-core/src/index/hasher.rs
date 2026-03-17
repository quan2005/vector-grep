use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result};

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .with_context(|| format!("读取文件失败: {}", path.display()))?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
