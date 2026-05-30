//! Minimal NPY / NPZ reader — supports `float32` and `int32` dtypes.
//!
//! Used to load pre-encoded NeuCodec reference codes (int32 1-D arrays)
//! and, optionally, float32 data from NPZ archives.

use anyhow::{bail, Context, Result};
use std::{collections::HashMap, io::Read, path::Path};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dtype {
    Float32,
    Int32,
}

impl Dtype {
    fn from_descr(s: &str) -> Result<(Self, bool)> {
        let s = s.trim().trim_matches('\'').trim_matches('"');
        let big_endian = s.starts_with('>');
        let dtype = match s {
            "<f4" | "=f4" | "|f4" | ">f4" => Dtype::Float32,
            "<i4" | "=i4" | "|i4" | ">i4" => Dtype::Int32,
            "<u4" | "=u4" | "|u4" | ">u4" => Dtype::Int32,
            other => bail!("Unsupported dtype '{}' — only float32 / int32 are supported", other),
        };
        Ok((dtype, big_endian))
    }

    fn bytes(self) -> usize {
        4
    }
}

fn extract_header_field<'a>(header: &'a str, field: &str) -> Option<&'a str> {
    let key_sq = format!("'{}':", field);
    let key_dq = format!("\"{}\":", field);
    let start = header
        .find(key_sq.as_str())
        .map(|p| p + key_sq.len())
        .or_else(|| header.find(key_dq.as_str()).map(|p| p + key_dq.len()))?;
    let rest = header[start..].trim_start();
    if rest.starts_with('(') {
        let end = rest.find(')')?;
        Some(&rest[..end + 1])
    } else if rest.starts_with('\'') || rest.starts_with('"') {
        let quote = rest.chars().next()?;
        let inner = &rest[1..];
        let end = inner.find(quote)?;
        Some(&inner[..end])
    } else {
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim())
    }
}

fn parse_shape(s: &str) -> Result<Vec<usize>> {
    let inner = s.trim_start_matches('(').trim_end_matches(')');
    if inner.trim().is_empty() {
        return Ok(vec![]);
    }
    inner
        .split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<usize>().with_context(|| format!("Bad shape dim: '{t}'")))
        .collect()
}

/// A loaded NPY array.
pub enum NpyData {
    Float32 { shape: Vec<usize>, data: Vec<f32> },
    Int32 { shape: Vec<usize>, data: Vec<i32> },
}

impl NpyData {
    pub fn len(&self) -> usize {
        match self {
            Self::Float32 { data, .. } => data.len(),
            Self::Int32 { data, .. } => data.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn shape(&self) -> &[usize] {
        match self {
            Self::Float32 { shape, .. } => shape,
            Self::Int32 { shape, .. } => shape,
        }
    }

    pub fn into_i32(self) -> Result<Vec<i32>> {
        match self {
            Self::Int32 { data, .. } => Ok(data),
            Self::Float32 { data, .. } => Ok(data.into_iter().map(|f| f as i32).collect()),
        }
    }

    pub fn into_f32(self) -> Result<Vec<f32>> {
        match self {
            Self::Float32 { data, .. } => Ok(data),
            Self::Int32 { data, .. } => Ok(data.into_iter().map(|i| i as f32).collect()),
        }
    }
}

/// Parse a raw `.npy` byte buffer into an [`NpyData`].
pub fn parse_npy(raw: &[u8]) -> Result<NpyData> {
    if raw.len() < 10 || &raw[..6] != b"\x93NUMPY" {
        bail!("Not a valid NPY file (bad magic)");
    }
    let major = raw[6];
    let minor = raw[7];
    let (header_len, header_start) = match (major, minor) {
        (1, _) => (u16::from_le_bytes([raw[8], raw[9]]) as usize, 10),
        (2, _) => {
            if raw.len() < 12 {
                bail!("NPY v2 file too short");
            }
            (u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]) as usize, 12)
        }
        _ => bail!("Unsupported NPY version {}.{}", major, minor),
    };
    let header_end = header_start + header_len;
    if raw.len() < header_end {
        bail!("NPY file truncated in header");
    }
    let header = std::str::from_utf8(&raw[header_start..header_end]).context("NPY header is not valid UTF-8")?;

    let descr = extract_header_field(header, "descr").context("NPY header missing 'descr'")?;
    let (dtype, big_endian) = Dtype::from_descr(descr)?;

    let fortran = extract_header_field(header, "fortran_order")
        .unwrap_or("False")
        .trim()
        .to_ascii_lowercase();
    if fortran == "true" {
        bail!("Fortran-order arrays are not supported");
    }

    let shape_str = extract_header_field(header, "shape").context("NPY header missing 'shape'")?;
    let shape = parse_shape(shape_str.trim())?;
    let n: usize = shape.iter().product();

    let data_bytes = &raw[header_end..];
    let byte_size = n * dtype.bytes();
    if data_bytes.len() < byte_size {
        bail!(
            "NPY data section too short: expected {byte_size} bytes, got {}",
            data_bytes.len()
        );
    }

    match dtype {
        Dtype::Float32 => {
            let data: Vec<f32> = data_bytes[..byte_size]
                .chunks_exact(4)
                .map(|b| {
                    let arr = [b[0], b[1], b[2], b[3]];
                    if big_endian {
                        f32::from_be_bytes(arr)
                    } else {
                        f32::from_le_bytes(arr)
                    }
                })
                .collect();
            Ok(NpyData::Float32 { shape, data })
        }
        Dtype::Int32 => {
            let data: Vec<i32> = data_bytes[..byte_size]
                .chunks_exact(4)
                .map(|b| {
                    let arr = [b[0], b[1], b[2], b[3]];
                    if big_endian {
                        i32::from_be_bytes(arr)
                    } else {
                        i32::from_le_bytes(arr)
                    }
                })
                .collect();
            Ok(NpyData::Int32 { shape, data })
        }
    }
}

/// Load a `.npy` file and return an [`NpyData`].
pub fn load_npy(path: &Path) -> Result<NpyData> {
    let raw = std::fs::read(path).with_context(|| format!("Cannot read NPY file: {}", path.display()))?;
    parse_npy(&raw).with_context(|| format!("Failed to parse NPY: {}", path.display()))
}

/// Load a `.npy` file and return the data as a flat `Vec<i32>`.
pub fn load_npy_i32(path: &Path) -> Result<Vec<i32>> {
    load_npy(path)?.into_i32()
}

/// Write a 1-D `int32` array to a `.npy` file (NPY v1.0, `<i4`, C-contiguous).
pub fn write_npy_i32(path: &Path, data: &[i32]) -> Result<()> {
    let header_str = format!(
        "{{'descr': '<i4', 'fortran_order': False, 'shape': ({},), }}",
        data.len()
    );
    let raw_len = header_str.len() + 1;
    let padded_len = raw_len.div_ceil(64) * 64;
    let pad = padded_len - raw_len;
    let mut header = header_str;
    for _ in 0..pad {
        header.push(' ');
    }
    header.push('\n');

    let mut buf = Vec::with_capacity(10 + header.len() + data.len() * 4);
    buf.extend_from_slice(b"\x93NUMPY");
    buf.push(1);
    buf.push(0);
    buf.extend_from_slice(&(header.len() as u16).to_le_bytes());
    buf.extend_from_slice(header.as_bytes());
    for &v in data {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    std::fs::write(path, &buf).with_context(|| format!("Cannot write NPY: {}", path.display()))
}

/// Load an NPZ file and return all arrays keyed by name (`.npy` extension stripped).
pub fn load_npz(path: &Path) -> Result<HashMap<String, NpyData>> {
    let file = std::fs::File::open(path).with_context(|| format!("Cannot open NPZ: {}", path.display()))?;
    let mut archive = ZipArchive::new(file).with_context(|| format!("Cannot open ZIP archive: {}", path.display()))?;
    let mut arrays = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("Failed to read ZIP entry")?;
        let name = entry.name().trim_end_matches(".npy").to_string();
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).context("Failed to read NPY entry")?;
        let arr = parse_npy(&buf).with_context(|| format!("Failed to parse NPY entry '{name}'"))?;
        arrays.insert(name, arr);
    }
    Ok(arrays)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_npy_i32(values: &[i32]) -> Vec<u8> {
        let n = values.len();
        let header_str = format!("{{'descr': '<i4', 'fortran_order': False, 'shape': ({n},), }}");
        let raw_len = header_str.len() + 1;
        let padded_len = raw_len.div_ceil(64) * 64;
        let pad_needed = padded_len - raw_len;
        let mut header = header_str;
        for _ in 0..pad_needed {
            header.push(' ');
        }
        header.push('\n');
        let mut buf = Vec::new();
        buf.extend_from_slice(b"\x93NUMPY");
        buf.push(1);
        buf.push(0);
        buf.extend_from_slice(&(header.len() as u16).to_le_bytes());
        buf.extend_from_slice(header.as_bytes());
        for &v in values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf
    }

    #[test]
    fn test_parse_i32() {
        let vals = vec![0i32, 5, 42, 1023];
        let buf = make_npy_i32(&vals);
        let data = parse_npy(&buf).unwrap().into_i32().unwrap();
        assert_eq!(data, vals);
    }

    #[test]
    fn test_bad_magic() {
        assert!(parse_npy(b"NOTANPY").is_err());
    }
}
