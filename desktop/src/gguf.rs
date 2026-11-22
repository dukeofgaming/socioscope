use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Header of a GGUF file. Tensor bytes stay on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelHeader {
    pub version: u32,
    pub architecture: String,
    pub name: String,
    pub tensors: usize,
    pub bytes: u64,
}

pub fn load_header(path: &Path) -> Result<ModelHeader, String> {
    let mut file = File::open(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let bytes = file.metadata().map_err(|err| err.to_string())?.len();
    let mut reader = Reader { file: &mut file, pos: 0, len: bytes };
    let magic = reader.u32().map_err(|err| format!("{}: {err}", path.display()))?;
    if magic != 0x4655_4747 {
        return Err(format!("{}: not a GGUF file", path.display()));
    }
    let version = reader.u32()?;
    if version < 2 || version > 3 {
        return Err(format!("{}: GGUF version {version} is not 2 or 3", path.display()));
    }
    let tensor_count = reader.u64()?;
    let kv_count = reader.u64()?;
    if tensor_count > 100_000 || kv_count > 100_000 {
        return Err(format!("{}: header counts are not plausible", path.display()));
    }
    let mut architecture = String::new();
    let mut name = String::new();
    for _ in 0..kv_count {
        let key = reader.string()?;
        let ty = reader.u32()?;
        let value = reader.value(ty, &key)?;
        if key == "general.architecture" {
            architecture = value;
        } else if key == "general.name" {
            name = value;
        }
    }
    for _ in 0..tensor_count {
        let _tensor = reader.string()?;
        let dims = reader.u32()?;
        if dims == 0 || dims > 8 {
            return Err(format!("{}: a tensor has {dims} dimensions", path.display()));
        }
        for _ in 0..dims {
            reader.u64()?;
        }
        reader.u32()?;
        reader.u64()?;
    }
    if reader.pos > bytes {
        return Err(format!("{}: header runs past the end of the file", path.display()));
    }
    Ok(ModelHeader {
        version,
        architecture,
        name,
        tensors: tensor_count as usize,
        bytes,
    })
}

pub fn is_laya(header: &ModelHeader) -> bool {
    header.architecture == "laya" || header.name.to_ascii_lowercase().contains("laya")
}

pub fn is_bonsai(header: &ModelHeader) -> bool {
    let arch = header.architecture.to_ascii_lowercase();
    let name = header.name.to_ascii_lowercase();
    arch.contains("bonsai")
        || name.contains("bonsai")
        || arch.contains("qwen")
        || name.contains("qwen")
}

struct Reader<'a> {
    file: &'a mut File,
    pos: u64,
    len: u64,
}

impl Reader<'_> {
    fn take(&mut self, n: u64) -> Result<Vec<u8>, String> {
        if self.pos.saturating_add(n) > self.len {
            return Err("header runs past the end of the file".into());
        }
        let mut buf = vec![0u8; n as usize];
        self.file.read_exact(&mut buf).map_err(|err| err.to_string())?;
        self.pos += n;
        Ok(buf)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u64()?;
        if n > 32 * 1024 * 1024 {
            return Err("a header string is larger than 32MB".into());
        }
        let bytes = self.take(n)?;
        String::from_utf8(bytes).map_err(|_| "a header string is not utf-8".into())
    }

    fn value(&mut self, ty: u32, key: &str) -> Result<String, String> {
        match ty {
            0 | 1 | 7 => {
                self.take(1)?;
                Ok(String::new())
            }
            2 | 3 => {
                self.take(2)?;
                Ok(String::new())
            }
            4 | 5 | 6 => {
                self.take(4)?;
                Ok(String::new())
            }
            8 => self.string(),
            10 | 11 | 12 => {
                self.take(8)?;
                Ok(String::new())
            }
            9 => {
                let elem = self.u32()?;
                let count = self.u64()?;
                if count > 5_000_000 {
                    return Err(format!("{key}: header array is too large"));
                }
                for _ in 0..count {
                    self.value(elem, key)?;
                }
                Ok(String::new())
            }
            other => Err(format!("{key}: unknown GGUF value type {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn put_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn put_u64(buf: &mut Vec<u8>, value: u64) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn put_str(buf: &mut Vec<u8>, value: &str) {
        put_u64(buf, value.len() as u64);
        buf.extend_from_slice(value.as_bytes());
    }

    fn sample(architecture: &str, name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        put_u32(&mut buf, 0x4655_4747);
        put_u32(&mut buf, 3);
        put_u64(&mut buf, 1);
        put_u64(&mut buf, 2);
        put_str(&mut buf, "general.architecture");
        put_u32(&mut buf, 8);
        put_str(&mut buf, architecture);
        put_str(&mut buf, "general.name");
        put_u32(&mut buf, 8);
        put_str(&mut buf, name);
        put_str(&mut buf, "token_embd.weight");
        put_u32(&mut buf, 2);
        put_u64(&mut buf, 4);
        put_u64(&mut buf, 4);
        put_u32(&mut buf, 1);
        put_u64(&mut buf, 0);
        buf.extend_from_slice(&[0u8; 32]);
        buf
    }

    #[test]
    fn loads_a_laya_header_and_rejects_a_plain_file() {
        let dir = std::env::temp_dir().join(format!("socioscope-gguf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("laya.gguf");
        std::fs::File::create(&path).unwrap().write_all(&sample("laya", "Laya")).unwrap();
        let header = load_header(&path).unwrap();
        assert_eq!(header.architecture, "laya");
        assert_eq!(header.tensors, 1);
        assert!(is_laya(&header));
        assert!(!is_bonsai(&header));

        let bonsai = dir.join("bonsai.gguf");
        std::fs::File::create(&bonsai)
            .unwrap()
            .write_all(&sample("qwen35", "Ternary Bonsai 2 2B"))
            .unwrap();
        let header = load_header(&bonsai).unwrap();
        assert!(is_bonsai(&header));

        let junk = dir.join("notes.txt");
        std::fs::write(&junk, b"hello").unwrap();
        assert!(load_header(&junk).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
