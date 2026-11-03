//! Minimal KTX 2.0 writer: 2D textures and cubemaps with a full mip chain,
//! block-compressed or uncompressed, with optional Zstandard
//! supercompression. Spec: https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html

use anyhow::{Result, ensure};

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const HEADER_LEN: usize = 80;
const LEVEL_INDEX_ENTRY_LEN: usize = 24;
const SUPERCOMPRESSION_NONE: u32 = 0;
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// Formats we write, with their Vulkan format ids and DFD descriptions.
/// Uncompressed formats are used by tests and by future LUT/UI imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Format {
    Bc4Unorm,
    Bc5Unorm,
    Bc6hUfloat,
    Bc7Unorm,
    Bc7Srgb,
    Rgba8Unorm,
    Rgba8Srgb,
    Rgba16Sfloat,
}

// Data Format Descriptor constants (Khronos Data Format Specification 1.3).
const MODEL_RGBSDA: u8 = 1;
const MODEL_BC4: u8 = 131;
const MODEL_BC5: u8 = 132;
const MODEL_BC6H: u8 = 133;
const MODEL_BC7: u8 = 134;
const PRIMARIES_BT709: u8 = 1;
const TRANSFER_LINEAR: u8 = 1;
const TRANSFER_SRGB: u8 = 2;
const QUAL_LINEAR: u8 = 0x10;
const QUAL_SIGNED: u8 = 0x40;
const QUAL_FLOAT: u8 = 0x80;
const CHANNEL_ALPHA: u8 = 15;

struct Sample {
    channel: u8,
    bit_offset: u16,
    bit_length: u8,
    lower: u32,
    upper: u32,
}

impl Format {
    pub fn vk_format(self) -> u32 {
        match self {
            Format::Bc4Unorm => 139,
            Format::Bc5Unorm => 141,
            Format::Bc6hUfloat => 143,
            Format::Bc7Unorm => 145,
            Format::Bc7Srgb => 146,
            Format::Rgba8Unorm => 37,
            Format::Rgba8Srgb => 43,
            Format::Rgba16Sfloat => 97,
        }
    }

    pub fn is_block_compressed(self) -> bool {
        !matches!(
            self,
            Format::Rgba8Unorm | Format::Rgba8Srgb | Format::Rgba16Sfloat
        )
    }

    /// Bytes per 4×4 block (compressed) or per texel (uncompressed).
    pub fn block_bytes(self) -> usize {
        match self {
            Format::Bc4Unorm => 8,
            Format::Bc5Unorm | Format::Bc6hUfloat | Format::Bc7Unorm | Format::Bc7Srgb => 16,
            Format::Rgba8Unorm | Format::Rgba8Srgb => 4,
            Format::Rgba16Sfloat => 8,
        }
    }

    /// Size in bytes of one image (face) of `w`×`h` texels.
    pub fn image_size(self, w: u32, h: u32) -> usize {
        if self.is_block_compressed() {
            (w.div_ceil(4) * h.div_ceil(4)) as usize * self.block_bytes()
        } else {
            (w * h) as usize * self.block_bytes()
        }
    }

    fn type_size(self) -> u32 {
        match self {
            Format::Rgba16Sfloat => 2,
            _ => 1,
        }
    }

    fn dfd(self) -> Vec<u8> {
        let srgb = matches!(self, Format::Bc7Srgb | Format::Rgba8Srgb);
        let transfer = if srgb { TRANSFER_SRGB } else { TRANSFER_LINEAR };
        let (model, dims, samples): (u8, [u8; 4], Vec<Sample>) = match self {
            Format::Bc4Unorm => (MODEL_BC4, [3, 3, 0, 0], vec![bc_sample(0, 0, 64)]),
            Format::Bc5Unorm => (
                MODEL_BC5,
                [3, 3, 0, 0],
                vec![bc_sample(0, 0, 64), bc_sample(1, 64, 64)],
            ),
            Format::Bc6hUfloat => (
                MODEL_BC6H,
                [3, 3, 0, 0],
                vec![Sample {
                    channel: QUAL_FLOAT,
                    bit_offset: 0,
                    bit_length: 127,
                    lower: 0,
                    upper: 1.0f32.to_bits(),
                }],
            ),
            Format::Bc7Unorm | Format::Bc7Srgb => {
                (MODEL_BC7, [3, 3, 0, 0], vec![bc_sample(0, 0, 128)])
            }
            Format::Rgba8Unorm | Format::Rgba8Srgb => (
                MODEL_RGBSDA,
                [0; 4],
                (0..4u8)
                    .map(|i| Sample {
                        channel: if i == 3 {
                            CHANNEL_ALPHA | if srgb { QUAL_LINEAR } else { 0 }
                        } else {
                            i
                        },
                        bit_offset: i as u16 * 8,
                        bit_length: 7,
                        lower: 0,
                        upper: 255,
                    })
                    .collect(),
            ),
            Format::Rgba16Sfloat => (
                MODEL_RGBSDA,
                [0; 4],
                (0..4u8)
                    .map(|i| Sample {
                        channel: (if i == 3 { CHANNEL_ALPHA } else { i })
                            | QUAL_FLOAT
                            | QUAL_SIGNED,
                        bit_offset: i as u16 * 16,
                        bit_length: 15,
                        lower: (-1.0f32).to_bits(),
                        upper: 1.0f32.to_bits(),
                    })
                    .collect(),
            ),
        };
        let bytes_plane0 = self.block_bytes() as u8;
        let block_size = 24 + 16 * samples.len() as u32;
        let mut out = Vec::new();
        put_u32(&mut out, 4 + block_size); // dfdTotalSize
        put_u32(&mut out, 0); // vendorId = Khronos, descriptorType = basic
        put_u32(&mut out, (block_size << 16) | 2); // versionNumber 2
        put_u32(
            &mut out,
            u32::from_le_bytes([model, PRIMARIES_BT709, transfer, 0]),
        );
        put_u32(&mut out, u32::from_le_bytes(dims));
        put_u32(&mut out, u32::from_le_bytes([bytes_plane0, 0, 0, 0]));
        put_u32(&mut out, 0);
        for s in samples {
            put_u32(
                &mut out,
                s.bit_offset as u32 | ((s.bit_length as u32) << 16) | ((s.channel as u32) << 24),
            );
            put_u32(&mut out, 0); // sample positions
            put_u32(&mut out, s.lower);
            put_u32(&mut out, s.upper);
        }
        out
    }
}

fn bc_sample(channel: u8, bit_offset: u16, bits: u16) -> Sample {
    Sample {
        channel,
        bit_offset,
        bit_length: (bits - 1) as u8,
        lower: 0,
        upper: u32::MAX,
    }
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A texture ready to write: `levels[mip][face]` holds encoded image bytes.
#[derive(Debug, Clone)]
pub struct Texture {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub cubemap: bool,
    pub levels: Vec<Vec<Vec<u8>>>,
}

impl Texture {
    pub fn faces(&self) -> usize {
        if self.cubemap { 6 } else { 1 }
    }

    fn validate(&self) -> Result<()> {
        ensure!(!self.levels.is_empty(), "texture has no mip levels");
        ensure!(
            !self.cubemap || self.width == self.height,
            "cubemap faces must be square"
        );
        for (i, level) in self.levels.iter().enumerate() {
            ensure!(level.len() == self.faces(), "level {i}: wrong face count");
            let (w, h) = mip_size(self.width, self.height, i as u32);
            for face in level {
                ensure!(
                    face.len() == self.format.image_size(w, h),
                    "level {i}: expected {} bytes, got {}",
                    self.format.image_size(w, h),
                    face.len()
                );
            }
        }
        Ok(())
    }
}

pub fn mip_size(w: u32, h: u32, level: u32) -> (u32, u32) {
    ((w >> level).max(1), (h >> level).max(1))
}

pub fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Serializes a texture to KTX2 bytes. `zstd_level` of `None` writes
/// uncompressed level data.
pub fn write(tex: &Texture, zstd_level: Option<i32>, writer_name: &str) -> Result<Vec<u8>> {
    tex.validate()?;
    let level_count = tex.levels.len();

    // Level payloads (all faces concatenated), optionally supercompressed.
    let mut payloads = Vec::with_capacity(level_count);
    for level in &tex.levels {
        let raw: Vec<u8> = level.concat();
        let uncompressed_len = raw.len() as u64;
        let data = match zstd_level {
            Some(l) => zstd::bulk::compress(&raw, l)?,
            None => raw,
        };
        payloads.push((data, uncompressed_len));
    }

    let dfd = tex.format.dfd();
    let mut kvd = Vec::new();
    for (key, value) in [("KTXorientation", "rd"), ("KTXwriter", writer_name)] {
        let mut kv = Vec::new();
        kv.extend_from_slice(key.as_bytes());
        kv.push(0);
        kv.extend_from_slice(value.as_bytes());
        kv.push(0);
        put_u32(&mut kvd, kv.len() as u32);
        kvd.extend_from_slice(&kv);
        while !kvd.len().is_multiple_of(4) {
            kvd.push(0);
        }
    }

    let dfd_offset = HEADER_LEN + LEVEL_INDEX_ENTRY_LEN * level_count;
    let kvd_offset = dfd_offset + dfd.len();
    let mut data_start = kvd_offset + kvd.len();
    // Without supercompression, levels must align to lcm(texel block size, 4).
    let align = if zstd_level.is_some() {
        1
    } else {
        lcm(tex.format.block_bytes(), 4)
    };

    // Mip data is stored smallest level first.
    let mut offsets = vec![0u64; level_count];
    let mut body = Vec::new();
    data_start = data_start.next_multiple_of(align.max(1));
    for i in (0..level_count).rev() {
        while !(data_start + body.len()).is_multiple_of(align) {
            body.push(0);
        }
        offsets[i] = (data_start + body.len()) as u64;
        body.extend_from_slice(&payloads[i].0);
    }

    let mut out = Vec::with_capacity(data_start + body.len());
    out.extend_from_slice(&IDENTIFIER);
    put_u32(&mut out, tex.format.vk_format());
    put_u32(&mut out, tex.format.type_size());
    put_u32(&mut out, tex.width);
    put_u32(&mut out, tex.height);
    put_u32(&mut out, 0); // pixelDepth
    put_u32(&mut out, 0); // layerCount (not an array)
    put_u32(&mut out, tex.faces() as u32);
    put_u32(&mut out, level_count as u32);
    put_u32(
        &mut out,
        if zstd_level.is_some() {
            SUPERCOMPRESSION_ZSTD
        } else {
            SUPERCOMPRESSION_NONE
        },
    );
    put_u32(&mut out, dfd_offset as u32);
    put_u32(&mut out, dfd.len() as u32);
    put_u32(&mut out, kvd_offset as u32);
    put_u32(&mut out, kvd.len() as u32);
    put_u64(&mut out, 0); // sgdByteOffset
    put_u64(&mut out, 0); // sgdByteLength
    for (i, (data, uncompressed)) in payloads.iter().enumerate() {
        put_u64(&mut out, offsets[i]);
        put_u64(&mut out, data.len() as u64);
        put_u64(&mut out, *uncompressed);
    }
    debug_assert_eq!(out.len(), dfd_offset);
    out.extend_from_slice(&dfd);
    out.extend_from_slice(&kvd);
    while out.len() < data_start {
        out.push(0);
    }
    out.extend_from_slice(&body);
    Ok(out)
}

fn lcm(a: usize, b: usize) -> usize {
    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    a / gcd(a, b) * b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
    }
    fn u64_at(b: &[u8], off: usize) -> u64 {
        u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
    }

    fn rgba_tex(w: u32, h: u32) -> Texture {
        let levels = (0..mip_count(w, h))
            .map(|i| {
                let (mw, mh) = mip_size(w, h, i);
                vec![vec![i as u8; (mw * mh * 4) as usize]]
            })
            .collect();
        Texture {
            format: Format::Rgba8Srgb,
            width: w,
            height: h,
            cubemap: false,
            levels,
        }
    }

    #[test]
    fn header_and_level_index_are_consistent() {
        let tex = rgba_tex(8, 4);
        for zstd in [None, Some(3)] {
            let bytes = write(&tex, zstd, "test").unwrap();
            assert_eq!(&bytes[..12], &IDENTIFIER);
            assert_eq!(u32_at(&bytes, 12), 43);
            assert_eq!(u32_at(&bytes, 20), 8);
            assert_eq!(u32_at(&bytes, 24), 4);
            assert_eq!(u32_at(&bytes, 36), 1); // faces
            assert_eq!(u32_at(&bytes, 40), 4); // levels: 8x4, 4x2, 2x1, 1x1
            let mut prev_offset = u64::MAX;
            for i in 0..4 {
                let entry = HEADER_LEN + i * LEVEL_INDEX_ENTRY_LEN;
                let off = u64_at(&bytes, entry) as usize;
                let len = u64_at(&bytes, entry + 8) as usize;
                let raw_len = u64_at(&bytes, entry + 16) as usize;
                let (w, h) = mip_size(8, 4, i as u32);
                assert_eq!(raw_len, (w * h * 4) as usize);
                assert!(off + len <= bytes.len());
                // Smaller mips are stored first, so offsets decrease with level index.
                assert!((off as u64) < prev_offset);
                prev_offset = off as u64;
                let data = &bytes[off..off + len];
                let raw = match zstd {
                    Some(_) => zstd::bulk::decompress(data, raw_len).unwrap(),
                    None => data.to_vec(),
                };
                assert!(raw.iter().all(|&b| b == i as u8));
            }
            // DFD total size matches the recorded length.
            let dfd_off = u32_at(&bytes, 48) as usize;
            assert_eq!(
                u32_at(&bytes, dfd_off) as usize,
                u32_at(&bytes, 52) as usize
            );
        }
    }

    #[test]
    fn rejects_wrong_sizes() {
        let mut tex = rgba_tex(4, 4);
        tex.levels[0][0].pop();
        assert!(write(&tex, None, "t").is_err());
    }

    #[test]
    fn block_sizes() {
        assert_eq!(Format::Bc7Srgb.image_size(4, 4), 16);
        assert_eq!(Format::Bc7Srgb.image_size(5, 5), 64);
        assert_eq!(Format::Bc4Unorm.image_size(1, 1), 8);
        assert_eq!(mip_count(1024, 512), 11);
        assert_eq!(mip_count(1, 1), 1);
    }
}
