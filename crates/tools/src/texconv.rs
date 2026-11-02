//! Texture import: PNG/JPG/HDR -> KTX2 with mipmaps and GPU block
//! compression (BC4/BC5/BC6H/BC7), which Windows GPUs and Apple Silicon
//! Macs both sample natively.

use crate::ktx2::{self, Format, Texture, mip_count, mip_size};
use anyhow::{Context, Result, bail};
use game_core::half::f32_to_f16_bits;
use intel_tex_2::{RSurface, RgSurface, RgbaSurface, bc4, bc5, bc6h, bc7};
use rayon::prelude::*;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// Albedo/emissive: sRGB, BC7.
    Color,
    /// Linear data in RGBA (e.g. packed ORM): BC7.
    Linear,
    /// Tangent-space normal map (OpenGL convention): BC5 from RG.
    Normal,
    /// Single-channel mask (AO, height): BC4 from R.
    Mask,
    /// Grayscale roughness packed into G (Bevy's metallic-roughness layout), BC7.
    Roughness,
    /// HDR 2D texture: BC6H.
    Hdr,
    /// Equirectangular HDR panorama -> cubemap, BC6H.
    HdrCubemap,
}

pub struct Options {
    pub kind: Kind,
    /// Downscale so the largest side is at most this (power of two for cubemaps).
    pub max_size: Option<u32>,
    pub zstd_level: Option<i32>,
}

/// Linear float RGBA image.
#[derive(Clone)]
struct ImageF {
    w: u32,
    h: u32,
    px: Vec<[f32; 4]>,
}

impl ImageF {
    fn get(&self, x: u32, y: u32) -> [f32; 4] {
        self.px[(y.min(self.h - 1) * self.w + x.min(self.w - 1)) as usize]
    }

    /// 2×2 box downsample (odd edges clamp).
    fn half(&self, renormalize_normals: bool) -> ImageF {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut px = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.get(x * 2 + dx, y * 2 + dy);
                    for c in 0..4 {
                        acc[c] += p[c] * 0.25;
                    }
                }
                if renormalize_normals {
                    let (nx, ny) = (acc[0] * 2.0 - 1.0, acc[1] * 2.0 - 1.0);
                    let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
                    acc[0] = (nx / len) * 0.5 + 0.5;
                    acc[1] = (ny / len) * 0.5 + 0.5;
                }
                px.push(acc);
            }
        }
        ImageF { w, h, px }
    }

    fn resize_to_fit(self, max: u32, renormalize: bool) -> ImageF {
        let mut img = self;
        while img.w.max(img.h) > max {
            img = img.half(renormalize);
        }
        img
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn load(path: &Path, kind: Kind) -> Result<ImageF> {
    // Detect the format from content: downloaded files may carry a wrong extension.
    let img = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .with_context(|| format!("reading {}", path.display()))?
        .decode()
        .with_context(|| format!("decoding {}", path.display()))?;
    let (w, h) = (img.width(), img.height());
    let px: Vec<[f32; 4]> = match kind {
        Kind::Hdr | Kind::HdrCubemap => img.to_rgba32f().pixels().map(|p| p.0).collect(),
        Kind::Color => img
            .to_rgba8()
            .pixels()
            .map(|p| {
                let c = |v: u8| srgb_to_linear(v as f32 / 255.0);
                [c(p[0]), c(p[1]), c(p[2]), p[3] as f32 / 255.0]
            })
            .collect(),
        _ => img
            .to_rgba8()
            .pixels()
            .map(|p| p.0.map(|v| v as f32 / 255.0))
            .collect(),
    };
    Ok(ImageF { w, h, px })
}

/// Pads to a multiple of 4 in both dimensions by edge replication.
fn padded(img: &ImageF) -> ImageF {
    let (w, h) = (img.w.next_multiple_of(4), img.h.next_multiple_of(4));
    if (w, h) == (img.w, img.h) {
        return img.clone();
    }
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            px.push(img.get(x, y));
        }
    }
    ImageF { w, h, px }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn encode(img: &ImageF, format: Format) -> Vec<u8> {
    let p = padded(img);
    let (w, h) = (p.w, p.h);
    let out = match format {
        Format::Bc7Srgb | Format::Bc7Unorm => {
            let srgb = format == Format::Bc7Srgb;
            let has_alpha = p.px.iter().any(|c| c[3] < 0.999);
            let data: Vec<u8> =
                p.px.iter()
                    .flat_map(|c| {
                        let f = |v: f32| {
                            if srgb {
                                to_u8(linear_to_srgb(v))
                            } else {
                                to_u8(v)
                            }
                        };
                        [f(c[0]), f(c[1]), f(c[2]), to_u8(c[3])]
                    })
                    .collect();
            let settings = if has_alpha {
                bc7::alpha_basic_settings()
            } else {
                bc7::opaque_basic_settings()
            };
            bc7::compress_blocks(
                &settings,
                &RgbaSurface {
                    data: &data,
                    width: w,
                    height: h,
                    stride: w * 4,
                },
            )
        }
        Format::Bc5Unorm => {
            let data: Vec<u8> =
                p.px.iter()
                    .flat_map(|c| [to_u8(c[0]), to_u8(c[1])])
                    .collect();
            bc5::compress_blocks(&RgSurface {
                data: &data,
                width: w,
                height: h,
                stride: w * 2,
            })
        }
        Format::Bc4Unorm => {
            let data: Vec<u8> = p.px.iter().map(|c| to_u8(c[0])).collect();
            bc4::compress_blocks(&RSurface {
                data: &data,
                width: w,
                height: h,
                stride: w,
            })
        }
        Format::Bc6hUfloat => {
            let data: Vec<u8> =
                p.px.iter()
                    .flat_map(|c| {
                        let mut b = [0u8; 8];
                        for (i, v) in [c[0], c[1], c[2], 1.0].into_iter().enumerate() {
                            b[i * 2..i * 2 + 2]
                                .copy_from_slice(&f32_to_f16_bits(v.max(0.0)).to_le_bytes());
                        }
                        b
                    })
                    .collect();
            bc6h::compress_blocks(
                &bc6h::basic_settings(),
                &RgbaSurface {
                    data: &data,
                    width: w,
                    height: h,
                    stride: w * 8,
                },
            )
        }
        Format::Rgba8Srgb | Format::Rgba8Unorm | Format::Rgba16Sfloat => {
            unreachable!("uncompressed formats are not produced by texconv")
        }
    };
    // `padded` may have grown the image, but block counts already match the
    // unpadded size rounded up to whole blocks.
    debug_assert_eq!(out.len(), format.image_size(img.w, img.h));
    out
}

/// Cubemap face direction (wgpu layer order +X, -X, +Y, -Y, +Z, -Z).
fn face_dir(face: usize, u: f32, v: f32) -> [f32; 3] {
    match face {
        0 => [1.0, -v, -u],
        1 => [-1.0, -v, u],
        2 => [u, 1.0, v],
        3 => [u, -1.0, -v],
        4 => [u, -v, 1.0],
        _ => [-u, -v, -1.0],
    }
}

fn sample_bilinear(img: &ImageF, x: f32, y: f32) -> [f32; 4] {
    let x0 = x.floor();
    let y0 = y.floor();
    let (fx, fy) = (x - x0, y - y0);
    let wrap = |v: f32| (v as i64).rem_euclid(img.w as i64) as u32;
    let clampy = |v: f32| (v.max(0.0) as u32).min(img.h - 1);
    let p00 = img.get(wrap(x0), clampy(y0));
    let p10 = img.get(wrap(x0 + 1.0), clampy(y0));
    let p01 = img.get(wrap(x0), clampy(y0 + 1.0));
    let p11 = img.get(wrap(x0 + 1.0), clampy(y0 + 1.0));
    let mut out = [0.0; 4];
    for c in 0..4 {
        let top = p00[c] + (p10[c] - p00[c]) * fx;
        let bot = p01[c] + (p11[c] - p01[c]) * fx;
        out[c] = top + (bot - top) * fy;
    }
    out
}

fn equirect_to_face(src: &ImageF, face: usize, size: u32) -> ImageF {
    let mut px = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
            let v = 2.0 * (y as f32 + 0.5) / size as f32 - 1.0;
            let [dx, dy, dz] = face_dir(face, u, v);
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            let (dx, dy, dz) = (dx / len, dy / len, dz / len);
            let lon = dx.atan2(-dz); // 0 at -Z (panorama center)
            let lat = dy.clamp(-1.0, 1.0).acos(); // 0 at zenith
            let sx = (lon / std::f32::consts::TAU + 0.5) * src.w as f32 - 0.5;
            let sy = (lat / std::f32::consts::PI) * src.h as f32 - 0.5;
            px.push(sample_bilinear(src, sx, sy));
        }
    }
    ImageF {
        w: size,
        h: size,
        px,
    }
}

/// Converts one source image to KTX2 bytes.
pub fn convert(input: &Path, opts: &Options) -> Result<Vec<u8>> {
    let src = load(input, opts.kind)?;
    let (format, normals) = match opts.kind {
        Kind::Color => (Format::Bc7Srgb, false),
        Kind::Linear | Kind::Roughness => (Format::Bc7Unorm, false),
        Kind::Normal => (Format::Bc5Unorm, true),
        Kind::Mask => (Format::Bc4Unorm, false),
        Kind::Hdr | Kind::HdrCubemap => (Format::Bc6hUfloat, false),
    };
    let src = if opts.kind == Kind::Roughness {
        ImageF {
            px: src.px.iter().map(|c| [0.0, c[0], 0.0, 1.0]).collect(),
            ..src
        }
    } else {
        src
    };

    let (base_faces, cubemap): (Vec<ImageF>, bool) = if opts.kind == Kind::HdrCubemap {
        // Face size: a quarter of the panorama width, power of two.
        let mut size = (src.w / 4).max(4).next_power_of_two();
        if let Some(max) = opts.max_size {
            size = size.min(max.next_power_of_two());
        }
        let faces = (0..6)
            .into_par_iter()
            .map(|f| equirect_to_face(&src, f, size))
            .collect();
        (faces, true)
    } else {
        let img = match opts.max_size {
            Some(max) => src.resize_to_fit(max, normals),
            None => src,
        };
        (vec![img], false)
    };

    let (w, h) = (base_faces[0].w, base_faces[0].h);
    if w == 0 || h == 0 {
        bail!("empty image");
    }
    let levels_n = mip_count(w, h);
    // Build mip chains per face, then encode every (level, face) in parallel.
    let chains: Vec<Vec<ImageF>> = base_faces
        .into_iter()
        .map(|base| {
            let mut chain = vec![base];
            for _ in 1..levels_n {
                let next = chain.last().expect("non-empty").half(normals);
                chain.push(next);
            }
            chain
        })
        .collect();
    let levels: Vec<Vec<Vec<u8>>> = (0..levels_n as usize)
        .into_par_iter()
        .map(|l| {
            chains
                .iter()
                .map(|chain| {
                    let img = &chain[l];
                    debug_assert_eq!((img.w, img.h), mip_size(w, h, l as u32));
                    encode(img, format)
                })
                .collect()
        })
        .collect();

    let tex = Texture {
        format,
        width: w,
        height: h,
        cubemap,
        levels,
    };
    ktx2::write(
        &tex,
        opts.zstd_level,
        &format!("agtool {}", env!("CARGO_PKG_VERSION")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, 255])
        });
        img.save(path).unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agtool-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn converts_every_ldr_kind_with_npot_size() {
        let input = tmp("in.png");
        write_png(&input, 37, 20);
        for kind in [
            Kind::Color,
            Kind::Linear,
            Kind::Normal,
            Kind::Mask,
            Kind::Roughness,
        ] {
            let bytes = convert(
                &input,
                &Options {
                    kind,
                    max_size: None,
                    zstd_level: Some(3),
                },
            )
            .unwrap();
            assert_eq!(&bytes[..4], &[0xAB, 0x4B, 0x54, 0x58]);
            let levels = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
            assert_eq!(levels, mip_count(37, 20));
        }
    }

    #[test]
    fn hdr_panorama_becomes_cubemap() {
        let input = tmp("pano.hdr");
        let img = image::Rgb32FImage::from_fn(64, 32, |_, y| {
            image::Rgb([1.0 + y as f32 * 0.1, 0.5, 0.25])
        });
        image::DynamicImage::ImageRgb32F(img).save(&input).unwrap();
        let bytes = convert(
            &input,
            &Options {
                kind: Kind::HdrCubemap,
                max_size: None,
                zstd_level: Some(3),
            },
        )
        .unwrap();
        let u = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(u(12), 143); // BC6H ufloat
        assert_eq!(u(20), 16); // face size = width / 4
        assert_eq!(u(36), 6); // faces
    }

    /// Parse our output with the `ktx2` crate (what Bevy's loader uses) and
    /// check header, level sizes and supercompressed payloads.
    fn check_with_ktx2_crate(bytes: &[u8], vk_format: u32, faces: u32) {
        let reader = ::ktx2::Reader::new(bytes).expect("ktx2 crate parses our file");
        let header = reader.header();
        assert_eq!(header.format.map(|f| f.value()), Some(vk_format));
        assert_eq!(header.face_count, faces);
        assert_eq!(
            header.supercompression_scheme,
            Some(::ktx2::SupercompressionScheme::Zstandard)
        );
        let fmt = match vk_format {
            146 => Format::Bc7Srgb,
            141 => Format::Bc5Unorm,
            143 => Format::Bc6hUfloat,
            other => panic!("unexpected format {other}"),
        };
        for (i, level) in reader.levels().enumerate() {
            let raw = zstd::bulk::decompress(level.data, level.uncompressed_byte_length as usize)
                .expect("level decompresses");
            let (w, h) = mip_size(header.pixel_width, header.pixel_height, i as u32);
            assert_eq!(
                raw.len(),
                fmt.image_size(w, h) * faces as usize,
                "level {i}"
            );
        }
        assert_eq!(reader.levels().count() as u32, header.level_count);
        // The basic DFD must parse and agree with the format's color model.
        assert!(reader.basic_dfd().is_some(), "basic DFD present");
        let expected_model = match fmt {
            Format::Bc7Srgb => ::ktx2::ColorModel::BC7,
            Format::Bc5Unorm => ::ktx2::ColorModel::BC5,
            _ => ::ktx2::ColorModel::BC6H,
        };
        assert_eq!(reader.color_model(), Some(expected_model));
        let srgb = fmt == Format::Bc7Srgb;
        assert_eq!(
            reader.transfer_function(),
            Some(if srgb {
                ::ktx2::TransferFunction::SRGB
            } else {
                ::ktx2::TransferFunction::Linear
            })
        );
    }

    #[test]
    fn output_is_readable_by_bevys_ktx2_parser() {
        let input = tmp("parse.png");
        write_png(&input, 64, 32);
        let opts = |kind| Options {
            kind,
            max_size: None,
            zstd_level: Some(3),
        };
        check_with_ktx2_crate(&convert(&input, &opts(Kind::Color)).unwrap(), 146, 1);
        check_with_ktx2_crate(&convert(&input, &opts(Kind::Normal)).unwrap(), 141, 1);
        let pano = tmp("parse.hdr");
        let img = image::Rgb32FImage::from_fn(64, 32, |_, _| image::Rgb([2.0, 1.0, 0.5]));
        image::DynamicImage::ImageRgb32F(img).save(&pano).unwrap();
        check_with_ktx2_crate(&convert(&pano, &opts(Kind::HdrCubemap)).unwrap(), 143, 6);
    }

    #[test]
    fn max_size_downscales() {
        let input = tmp("big.png");
        write_png(&input, 256, 128);
        let bytes = convert(
            &input,
            &Options {
                kind: Kind::Color,
                max_size: Some(64),
                zstd_level: None,
            },
        )
        .unwrap();
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 64);
    }
}
