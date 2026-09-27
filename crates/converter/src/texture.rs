use color_eyre::{
    Result,
    eyre::{WrapErr, ensure},
};
use ddsfile::{Caps2, D3DFormat, Dds, MiscFlag, PixelFormatFlags};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::c_void,
    fs::{self, File},
    io::{Cursor, Read},
    path::Path,
    sync::Once,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureSemantic {
    BaseColor,
    Normal,
    Emissive,
    MetallicRoughness,
    Occlusion,
    SpecularGlossiness,
    Height,
    Detail,
    EnvironmentCube,
    EnvironmentMask,
    InnerLayer,
    Greyscale,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureEncoding {
    ColorSrgb,
    NormalLinear,
    DataLinear,
}

impl TextureEncoding {
    pub fn from_semantics(semantics: &BTreeSet<TextureSemantic>) -> Result<Self> {
        let color = semantics.iter().any(|semantic| {
            matches!(
                semantic,
                TextureSemantic::BaseColor
                    | TextureSemantic::SpecularGlossiness
                    | TextureSemantic::Detail
                    | TextureSemantic::EnvironmentCube
            )
        });
        let emissive = semantics.contains(&TextureSemantic::Emissive);
        let normal = semantics.contains(&TextureSemantic::Normal);
        let data = semantics.iter().any(|semantic| {
            matches!(
                semantic,
                TextureSemantic::MetallicRoughness
                    | TextureSemantic::Occlusion
                    | TextureSemantic::Height
                    | TextureSemantic::EnvironmentMask
                    | TextureSemantic::InnerLayer
                    | TextureSemantic::Greyscale
            )
        });
        Ok(if normal {
            // A shared normal must stay linear and use the normal-map encoder;
            // sampling it as sRGB would corrupt its direction vectors.
            Self::NormalLinear
        } else if data {
            // Bethesda reuses some color/emissive images as masks or height
            // data. Linear encoding preserves those channel values.
            Self::DataLinear
        } else if color || emissive {
            Self::ColorSrgb
        } else {
            Self::DataLinear
        })
    }

    const fn is_srgb(self) -> bool {
        matches!(self, Self::ColorSrgb)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ktx2Metadata {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub layers: u32,
    pub faces: u32,
    pub levels: u32,
    pub encoding: TextureEncoding,
    pub format: String,
    pub supercompression: String,
    pub encoded_bytes: u64,
    pub expanded_rgba_bytes: u64,
    pub sha256: String,
}

const KTX2_IDENTIFIER: &[u8; 12] = b"\xABKTX 20\xBB\r\n\x1A\n";
const FLAG_KTX2: u32 = 1 << 11;
const FLAG_SRGB: u32 = 1 << 13;
const FLAG_GENERATE_MIPS_CLAMP: u32 = 1 << 14;
const FLAG_UASTC: u32 = 1 << 17;
const UASTC_LEVEL_DEFAULT: u8 = 2;
const ETC1S_QUALITY_DEFAULT: u8 = 192;
static BASIS_INIT: Once = Once::new();
type EncodedVolumeLevels = Vec<Vec<Vec<u8>>>;
type EncodedVolume = (EncodedVolumeLevels, Vec<u8>);

unsafe extern "C" {
    fn opensky_basis_compress_ktx2(
        rgba: *const u8,
        width: u32,
        height: u32,
        flags_and_quality: u32,
        uastc_rdo_quality: f32,
        size: *mut usize,
    ) -> *mut c_void;
    fn opensky_basis_free(data: *mut c_void);
}

pub struct TextureConverter;

impl TextureConverter {
    pub fn convert_dds_to_ktx2(
        input: &Path,
        output: &Path,
        encoding: TextureEncoding,
    ) -> Result<Ktx2Metadata> {
        Self::convert_dds_to_ktx2_with_options(
            input,
            output,
            encoding,
            ETC1S_QUALITY_DEFAULT,
            UASTC_LEVEL_DEFAULT,
        )
    }

    pub fn convert_dds_to_ktx2_with_options(
        input: &Path,
        output: &Path,
        encoding: TextureEncoding,
        etc1s_quality: u8,
        uastc_level: u8,
    ) -> Result<Ktx2Metadata> {
        let file =
            File::open(input).wrap_err_with(|| format!("failed to open {}", input.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .wrap_err_with(|| format!("failed to memory-map {}", input.display()))?;

        let ktx2 = Self::convert_with_options(&mmap, encoding, etc1s_quality, uastc_level)?;
        let metadata = inspect_ktx2(&ktx2, encoding)?;
        publish_ktx2(output, &ktx2)?;
        Ok(metadata)
    }

    pub fn convert(dds_bytes: &[u8], encoding: TextureEncoding) -> Result<Vec<u8>> {
        Self::convert_with_options(
            dds_bytes,
            encoding,
            ETC1S_QUALITY_DEFAULT,
            UASTC_LEVEL_DEFAULT,
        )
    }

    pub fn convert_with_options(
        dds_bytes: &[u8],
        encoding: TextureEncoding,
        etc1s_quality: u8,
        uastc_level: u8,
    ) -> Result<Vec<u8>> {
        let container = Self::encode_container(dds_bytes, encoding, etc1s_quality, uastc_level)?;
        let published = supercompress_ktx2(&container)?;
        // `encode_container` validated the container against the DDS; the rewrite only moves level
        // payloads and the fields that describe them, so the published bytes are re-checked here
        // for the contract the runtime sees.
        validate_ktx2(&published, encoding)?;
        ensure!(
            is_zstd_supercompressed(&published),
            "converted KTX2 does not carry Zstandard-supercompressed levels"
        );
        Ok(published)
    }

    /// Encodes every decoded level as raw UASTC in a KTX2 container, without supercompression.
    ///
    /// This is the container [`Self::convert_with_options`] rewrites for publication; tests use it
    /// as the uncompressed reference whose levels the published container must decode to.
    pub(crate) fn encode_container(
        dds_bytes: &[u8],
        encoding: TextureEncoding,
        etc1s_quality: u8,
        uastc_level: u8,
    ) -> Result<Vec<u8>> {
        let dds = Dds::read(Cursor::new(dds_bytes)).wrap_err("invalid DDS")?;
        let depth = dds.get_depth();
        let is_cubemap = dds.header.caps2.contains(Caps2::CUBEMAP)
            || dds
                .header10
                .as_ref()
                .is_some_and(|header| header.misc_flag.contains(MiscFlag::TEXTURECUBE));
        let layer_count = dds.get_num_array_layers();
        ensure!(
            layer_count <= 1 || (is_cubemap && layer_count == 6),
            "DDS texture arrays are not supported"
        );
        ensure!(
            !is_cubemap || layer_count == 6,
            "DDS cubemap does not contain exactly six faces"
        );
        if depth > 1 {
            ensure!(!is_cubemap, "DDS cannot be both a volume and a cubemap");
            ensure!(layer_count <= 1, "volume DDS arrays are not supported");
            let (encoded_levels, template) = match image_dds::SurfaceRgba8::decode_dds(&dds) {
                Ok(surface) => {
                    encode_decoded_volume(&surface, encoding, etc1s_quality, uastc_level)?
                }
                Err(_) if is_l8_volume(&dds) => {
                    encode_l8_volume(&dds, encoding, etc1s_quality, uastc_level)?
                }
                Err(error) => return Err(error).wrap_err("DDS volume cannot be decoded"),
            };
            let result = combine_ktx2_volume(
                &template,
                &encoded_levels,
                dds.get_width(),
                dds.get_height(),
                depth,
            )?;
            validate_ktx2_against_dds(&result, &dds, encoding, false)?;
            return Ok(result);
        }
        if is_cubemap {
            let mut encoded_faces = Vec::with_capacity(6);
            for face in 0..6 {
                let surface = image_dds::SurfaceRgba8::decode_layers_mipmaps_dds(
                    &dds,
                    face..face + 1,
                    0..dds.get_num_mipmap_levels(),
                )
                .wrap_err_with(|| format!("DDS cubemap face {face} cannot be decoded"))?;
                encoded_faces.push(encode_2d_surface(
                    &surface,
                    encoding,
                    etc1s_quality,
                    uastc_level,
                )?);
            }
            let result = combine_ktx2_cubemap_faces(&encoded_faces)?;
            validate_ktx2_against_dds(&result, &dds, encoding, true)?;
            return Ok(result);
        }

        let result = match image_dds::SurfaceRgba8::decode_dds(&dds) {
            Ok(surface) => encode_2d_surface(&surface, encoding, etc1s_quality, uastc_level)?,
            Err(_) if dds.get_d3d_format() == Some(D3DFormat::X8R8G8B8) => {
                encode_x8r8g8b8(&dds, encoding, etc1s_quality, uastc_level)?
            }
            Err(error) => return Err(error).wrap_err("DDS pixel format cannot be decoded"),
        };
        validate_ktx2_against_dds(&result, &dds, encoding, false)?;
        Ok(result)
    }
}

/// Rewrites a KTX2 container so every level is one Zstandard frame.
///
/// The Basis encoder emits UASTC levels as raw blocks (`supercompressionScheme = 0`), and the
/// runtime KTX2 loader decompresses a Zstandard-supercompressed container (scheme 2) before
/// transcoding it. Supercompression is lossless over the level data: each rewritten level decodes
/// to exactly the bytes the raw container held. Everything else — the DFD, key/value data,
/// dimensions, transfer function and sRGB alias handling — is copied through untouched; only the
/// level payloads, their index entries and the scheme field change. Levels keep their original
/// order and are laid out back to back: KTX2 asks supercompressed level data for no alignment.
pub fn supercompress_ktx2(bytes: &[u8]) -> Result<Vec<u8>> {
    // Only the header and the level table are read here. Parsing the whole container would run the
    // DFD parser over bytes this function copies through untouched, and a damaged descriptor is
    // not this step's to interpret.
    let mut header = bytes
        .get(..ktx2::Header::LENGTH)
        .and_then(|header| <&[u8; ktx2::Header::LENGTH]>::try_from(header).ok())
        .and_then(|header| ktx2::Header::from_bytes(header).ok())
        .ok_or_else(|| color_eyre::eyre::eyre!("not a KTX2 container"))?;
    ensure!(
        header.supercompression_scheme.is_none(),
        "KTX2 levels are already supercompressed ({:?})",
        header.supercompression_scheme
    );
    ensure!(
        header.index.sgd_byte_length == 0 && header.index.sgd_byte_offset == 0,
        "Zstandard supercompression does not use supercompression global data"
    );
    let level_count = header.level_count.max(1) as usize;
    let table_end = ktx2::Header::LENGTH
        .checked_add(
            level_count
                .checked_mul(ktx2::LevelIndex::LENGTH)
                .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 level table overflow"))?,
        )
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 level table overflow"))?;
    // The count comes from the file, so it is only trusted once the table it describes fits in the
    // container: every allocation and index below is bounded by that.
    ensure!(
        table_end <= bytes.len(),
        "KTX2 level table does not fit in the container"
    );
    let mut levels = Vec::with_capacity(level_count);
    for level in 0..level_count {
        let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
        let index = bytes
            .get(start..start + ktx2::LevelIndex::LENGTH)
            .and_then(|index| <&[u8; ktx2::LevelIndex::LENGTH]>::try_from(index).ok())
            .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 level {level} index is truncated"))?;
        let index = ktx2::LevelIndex::from_bytes(index);
        let end = (index.byte_offset as usize)
            .checked_add(index.byte_length as usize)
            .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 level {level} size overflow"))?;
        let data = bytes
            .get(index.byte_offset as usize..end)
            .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 level {level} is out of bounds"))?;
        levels.push((level, index.byte_offset as usize, data));
    }
    // The level table sits between the header and the DFD/KVD sections, so the smallest level
    // offset bounds the metadata that has to survive the rewrite unchanged.
    let metadata_end = (table_end as u64)
        .max(u64::from(header.index.dfd_byte_offset) + u64::from(header.index.dfd_byte_length))
        .max(u64::from(header.index.kvd_byte_offset) + u64::from(header.index.kvd_byte_length));
    let first_data_offset = levels
        .iter()
        .map(|(_, offset, _)| *offset)
        .min()
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 has no levels"))?;
    ensure!(
        first_data_offset as u64 >= metadata_end && first_data_offset <= bytes.len(),
        "invalid KTX2 metadata layout"
    );

    // Levels keep the order they had in the container. Supercompressed data carries no alignment
    // requirement in KTX2, so the payloads are laid out back to back and readers go through the
    // index instead.
    levels.sort_by_key(|(_, offset, _)| *offset);
    let mut offsets = vec![0u64; level_count];
    let mut lengths = vec![0u64; level_count];
    let mut uncompressed_lengths = vec![0u64; level_count];
    let mut output = bytes[..first_data_offset].to_vec();
    for (mip, _, data) in &levels {
        let compressed = ruzstd::encoding::compress_to_vec(
            Cursor::new(*data),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        offsets[*mip] = output.len() as u64;
        lengths[*mip] = compressed.len() as u64;
        uncompressed_lengths[*mip] = data.len() as u64;
        output.extend_from_slice(&compressed);
    }
    header.supercompression_scheme = Some(ktx2::SupercompressionScheme::Zstandard);
    output[..ktx2::Header::LENGTH].copy_from_slice(&header.as_bytes());
    for mip in 0..level_count {
        let index = ktx2::LevelIndex {
            byte_offset: offsets[mip],
            byte_length: lengths[mip],
            uncompressed_byte_length: uncompressed_lengths[mip],
        };
        let start = ktx2::Header::LENGTH + mip * ktx2::LevelIndex::LENGTH;
        output[start..start + ktx2::LevelIndex::LENGTH].copy_from_slice(&index.as_bytes());
    }
    Ok(output)
}

/// Writes the supercompressed runtime form of the published texture at `source` to `output`.
///
/// A texture published before this converter wrote Zstandard levels — or a staging directory left
/// by such a build — holds raw levels. Re-encoding UASTC from the source DDS takes minutes per
/// thousand textures; recompressing the levels of the published container takes milliseconds and
/// yields the bytes an encoding of the same DDS would have produced. `source` and `output` may
/// name the same file: every byte is read before anything is written.
///
/// The container is checked by its header and level table only. A published texture can hold a
/// damaged data format descriptor — the ktx2 crate's parser panics on one — and that must fail the
/// asset, not abort a conversion of thousands of them: the caller re-encodes from the source DDS.
pub fn supercompress_ktx2_file(source: &Path, output: &Path) -> Result<()> {
    let bytes =
        fs::read(source).wrap_err_with(|| format!("failed to read {}", source.display()))?;
    let published = supercompress_ktx2(&bytes)?;
    ensure!(
        is_zstd_supercompressed(&published),
        "rewritten KTX2 does not declare Zstandard"
    );
    // The rewrite never interprets the descriptor, so the published container is read here the way
    // the runtime reads it — including the parser that panics on a damaged descriptor. A bad file
    // has to fail its asset, not abort a conversion of thousands of them.
    let inspected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inspect_runtime_ktx2(&published)
    }))
    .map_err(|_| {
        color_eyre::eyre::eyre!(
            "KTX2 {} carries a damaged data format descriptor",
            source.display()
        )
    })?;
    inspected.wrap_err_with(|| format!("invalid runtime KTX2 {}", source.display()))?;
    publish_ktx2(output, &published)
}

/// True when `bytes` begins with a KTX2 header whose levels carry Zstandard supercompression.
///
/// Only the 80-byte header is read, so this can be asked of a whole file or of the first block of
/// one, and an unreadable or non-KTX2 file reports `false` instead of an error.
pub fn is_zstd_supercompressed(bytes: &[u8]) -> bool {
    bytes
        .get(..ktx2::Header::LENGTH)
        .and_then(|header| <&[u8; ktx2::Header::LENGTH]>::try_from(header).ok())
        .and_then(|header| ktx2::Header::from_bytes(header).ok())
        .is_some_and(|header| {
            header.supercompression_scheme == Some(ktx2::SupercompressionScheme::Zstandard)
        })
}

/// Reads only the KTX2 header of `path`; see [`is_zstd_supercompressed`].
pub fn file_is_zstd_supercompressed(path: &Path) -> bool {
    let mut header = [0u8; ktx2::Header::LENGTH];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|()| is_zstd_supercompressed(&header))
}

/// Decodes every level of a container the way the runtime does; test-only.
#[cfg(test)]
pub(crate) fn decode_ktx2_levels(bytes: &[u8]) -> Vec<Vec<u8>> {
    let reader = ktx2::Reader::new(bytes).expect("valid KTX2");
    reader
        .levels()
        .map(|level| match reader.header().supercompression_scheme {
            Some(ktx2::SupercompressionScheme::Zstandard) => {
                let mut decoded = Vec::new();
                ruzstd::decoding::StreamingDecoder::new(Cursor::new(level.data))
                    .expect("valid Zstandard frame")
                    .read_to_end(&mut decoded)
                    .expect("complete Zstandard frame");
                decoded
            }
            None => level.data.to_vec(),
            other => panic!("unsupported test supercompression {other:?}"),
        })
        .collect()
}

/// Puts `bytes` at `output`, replacing a previous file only once the new bytes are on disk.
///
/// The destination's parent directory is created when missing. A leftover publication file beside
/// `output` is an error rather than something to overwrite: the only writer of those names is a
/// crashed run of this function.
fn publish_ktx2(output: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension(format!("ktx2.{}.partial", std::process::id()));
    let backup = output.with_extension(format!("ktx2.{}.backup", std::process::id()));
    ensure!(
        !temporary.exists() && !backup.exists(),
        "stale texture publication file exists beside {}",
        output.display()
    );
    fs::write(&temporary, bytes)
        .wrap_err_with(|| format!("failed to write {}", temporary.display()))?;
    let publish = (|| {
        let had_previous = output.is_file();
        if had_previous {
            fs::rename(output, &backup).wrap_err_with(|| {
                format!("failed to stage replacement for {}", output.display())
            })?;
        }
        if let Err(error) = fs::rename(&temporary, output) {
            if had_previous {
                let _ = fs::rename(&backup, output);
            }
            return Err(error).wrap_err_with(|| format!("failed to publish {}", output.display()));
        }
        if had_previous {
            fs::remove_file(&backup).wrap_err_with(|| {
                format!("failed to remove publication backup {}", backup.display())
            })?;
        }
        Ok(())
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temporary);
        if backup.is_file() && !output.is_file() {
            let _ = fs::rename(&backup, output);
        }
    }
    publish
}

fn encode_2d_surface(
    surface: &image_dds::SurfaceRgba8<Vec<u8>>,
    encoding: TextureEncoding,
    etc1s_quality: u8,
    uastc_level: u8,
) -> Result<Vec<u8>> {
    ensure!(
        surface.layers == 1 && surface.depth == 1,
        "2D texture surface has an incompatible layout"
    );
    let mut encoded_levels = Vec::with_capacity(surface.mipmaps as usize);
    for mip in 0..surface.mipmaps {
        let image = surface
            .get_image(0, 0, mip)
            .ok_or_else(|| color_eyre::eyre::eyre!("DDS mip {mip} has invalid dimensions"))?;
        encoded_levels.push(encode_basis_ktx2(
            image.width(),
            image.height(),
            &image.into_raw(),
            encoding,
            false,
            etc1s_quality,
            uastc_level,
        )?);
    }
    if encoded_levels.len() == 1 {
        return Ok(encoded_levels.pop().expect("one encoded mip"));
    }
    let base = surface
        .get_image(0, 0, 0)
        .ok_or_else(|| color_eyre::eyre::eyre!("DDS has no base mip"))?;
    let template = encode_basis_ktx2(
        base.width(),
        base.height(),
        &base.into_raw(),
        encoding,
        true,
        etc1s_quality,
        uastc_level,
    )?;
    combine_ktx2_mip_levels(&template, &encoded_levels)
}

fn combine_ktx2_mip_levels(template: &[u8], levels: &[Vec<u8>]) -> Result<Vec<u8>> {
    ensure!(!levels.is_empty(), "KTX2 mip chain is empty");
    let template_reader = ktx2::Reader::new(template)
        .map_err(|error| color_eyre::eyre::eyre!("invalid KTX2 mip template: {error:?}"))?;
    let mut reference = template_reader.header();
    ensure!(
        reference.level_count as usize >= levels.len(),
        "generated KTX2 mip template has only {} levels, expected at least {}",
        reference.level_count,
        levels.len()
    );
    let level_count = levels.len();
    let first_data_offset = template_reader
        .levels()
        .enumerate()
        .map(|(level, _)| {
            let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
            let bytes: &[u8; ktx2::LevelIndex::LENGTH] = template
                [start..start + ktx2::LevelIndex::LENGTH]
                .try_into()
                .expect("fixed-size KTX2 level index");
            ktx2::LevelIndex::from_bytes(bytes).byte_offset as usize
        })
        .min()
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 mip template has no levels"))?;
    let mut output = template[..first_data_offset].to_vec();
    reference.level_count = u32::try_from(level_count).wrap_err("too many DDS mip levels")?;
    output[..ktx2::Header::LENGTH].copy_from_slice(&reference.as_bytes());
    let mut indexes = Vec::with_capacity(level_count);
    for (mip, level) in levels.iter().enumerate() {
        let reader = ktx2::Reader::new(level)
            .map_err(|error| color_eyre::eyre::eyre!("invalid encoded DDS mip {mip}: {error:?}"))?;
        let header = reader.header();
        ensure!(
            header.level_count == 1
                && header.face_count == 1
                && header.layer_count == 0
                && header.pixel_depth == 0
                && header.pixel_width == (reference.pixel_width >> mip).max(1)
                && header.pixel_height == (reference.pixel_height >> mip).max(1)
                && header.format == reference.format
                && header.supercompression_scheme == reference.supercompression_scheme,
            "encoded DDS mip {mip} has an incompatible layout"
        );
        while !output.len().is_multiple_of(16) {
            output.push(0);
        }
        let offset = output.len() as u64;
        let encoded = reader
            .levels()
            .next()
            .ok_or_else(|| color_eyre::eyre::eyre!("encoded DDS mip {mip} has no data"))?;
        output.extend_from_slice(encoded.data);
        indexes.push(ktx2::LevelIndex {
            byte_offset: offset,
            byte_length: encoded.data.len() as u64,
            uncompressed_byte_length: encoded.uncompressed_byte_length,
        });
    }
    for (level, index) in indexes.iter().enumerate() {
        let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
        output[start..start + ktx2::LevelIndex::LENGTH].copy_from_slice(&index.as_bytes());
    }
    Ok(output)
}

pub fn inspect_ktx2(bytes: &[u8], encoding: TextureEncoding) -> Result<Ktx2Metadata> {
    validate_ktx2(bytes, encoding)?;
    let reader = ktx2::Reader::new(bytes)
        .map_err(|error| color_eyre::eyre::eyre!("generated invalid KTX2: {error:?}"))?;
    let header = reader.header();
    let levels = header.level_count.max(1);
    ensure!(
        reader.levels().count() == levels as usize,
        "KTX2 level index is incomplete"
    );
    let faces = header.face_count.max(1);
    let layers = header.layer_count.max(1);
    let base_depth = header.pixel_depth.max(1);
    let mut expanded_rgba_bytes = 0u64;
    for mip in 0..levels {
        let width = (header.pixel_width >> mip).max(1) as u64;
        let height = (header.pixel_height.max(1) >> mip).max(1) as u64;
        let depth = (base_depth >> mip).max(1) as u64;
        let level_bytes = width
            .checked_mul(height)
            .and_then(|value| value.checked_mul(depth))
            .and_then(|value| value.checked_mul(u64::from(faces)))
            .and_then(|value| value.checked_mul(u64::from(layers)))
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 expanded size overflow"))?;
        expanded_rgba_bytes = expanded_rgba_bytes
            .checked_add(level_bytes)
            .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 expanded size overflow"))?;
    }
    Ok(Ktx2Metadata {
        width: header.pixel_width,
        height: header.pixel_height.max(1),
        depth: base_depth,
        layers,
        faces,
        levels,
        encoding,
        format: format!("{:?}", reader.color_model()),
        supercompression: format!("{:?}", header.supercompression_scheme),
        encoded_bytes: u64::try_from(bytes.len()).wrap_err("KTX2 size does not fit in u64")?,
        expanded_rgba_bytes,
        sha256: crate::cache::hash_bytes(bytes),
    })
}

/// Validates an already-published runtime KTX2 when its source material
/// semantic is unavailable. Linear textures are reported as data textures;
/// normal/data distinction does not change the runtime container contract.
pub fn inspect_runtime_ktx2(bytes: &[u8]) -> Result<Ktx2Metadata> {
    let reader = ktx2::Reader::new(bytes)
        .map_err(|error| color_eyre::eyre::eyre!("invalid runtime KTX2: {error:?}"))?;
    let encoding = match reader.transfer_function() {
        Some(ktx2::TransferFunction::SRGB) => TextureEncoding::ColorSrgb,
        Some(ktx2::TransferFunction::Linear) => TextureEncoding::DataLinear,
        transfer => color_eyre::eyre::bail!("KTX2 has unsupported transfer function {transfer:?}"),
    };
    inspect_ktx2(bytes, encoding)
}

fn validate_ktx2_against_dds(
    bytes: &[u8],
    dds: &Dds,
    encoding: TextureEncoding,
    cubemap: bool,
) -> Result<Ktx2Metadata> {
    let metadata = inspect_ktx2(bytes, encoding)?;
    ensure!(
        metadata.width == dds.get_width()
            && metadata.height == dds.get_height()
            && metadata.depth == dds.get_depth().max(1),
        "KTX2 dimensions {:?} do not match DDS {}x{}x{}",
        (metadata.width, metadata.height, metadata.depth),
        dds.get_width(),
        dds.get_height(),
        dds.get_depth().max(1)
    );
    ensure!(
        metadata.levels == dds.get_num_mipmap_levels().max(1),
        "KTX2 has {} levels, but DDS has {}",
        metadata.levels,
        dds.get_num_mipmap_levels().max(1)
    );
    ensure!(
        metadata.faces == if cubemap { 6 } else { 1 },
        "KTX2 face count does not match DDS"
    );
    ensure!(metadata.layers == 1, "KTX2 arrays are not supported");
    Ok(metadata)
}

fn encode_decoded_volume(
    surface: &image_dds::SurfaceRgba8<Vec<u8>>,
    encoding: TextureEncoding,
    etc1s_quality: u8,
    uastc_level: u8,
) -> Result<EncodedVolume> {
    let mut encoded_levels = Vec::with_capacity(surface.mipmaps as usize);
    for mip in 0..surface.mipmaps {
        let mip_depth = (surface.depth >> mip).max(1);
        let mut encoded_slices = Vec::with_capacity(mip_depth as usize);
        for slice in 0..mip_depth {
            let image = surface.get_image(0, slice, mip).ok_or_else(|| {
                color_eyre::eyre::eyre!("DDS volume mip {mip} slice {slice} has invalid dimensions")
            })?;
            encoded_slices.push(encode_basis_ktx2(
                image.width(),
                image.height(),
                &image.into_raw(),
                encoding,
                false,
                etc1s_quality,
                uastc_level,
            )?);
        }
        encoded_levels.push(encoded_slices);
    }
    let base_image = surface
        .get_image(0, 0, 0)
        .ok_or_else(|| color_eyre::eyre::eyre!("DDS volume has no base slice"))?;
    let template = encode_basis_ktx2(
        base_image.width(),
        base_image.height(),
        &base_image.into_raw(),
        encoding,
        surface.mipmaps > 1,
        etc1s_quality,
        uastc_level,
    )?;
    Ok((encoded_levels, template))
}

fn is_l8_volume(dds: &Dds) -> bool {
    dds.header.spf.flags.contains(PixelFormatFlags::LUMINANCE)
        && dds.header.spf.rgb_bit_count == Some(8)
        && !dds.header.spf.flags.contains(PixelFormatFlags::ALPHA)
        && !dds
            .header
            .spf
            .flags
            .contains(PixelFormatFlags::ALPHA_PIXELS)
}

fn encode_l8_volume(
    dds: &Dds,
    encoding: TextureEncoding,
    etc1s_quality: u8,
    uastc_level: u8,
) -> Result<EncodedVolume> {
    let mip_count = dds.get_num_mipmap_levels();
    let mut offset = 0usize;
    let mut encoded_levels = Vec::with_capacity(mip_count as usize);
    let mut base_rgba = None;
    for mip in 0..mip_count {
        let width = (dds.get_width() >> mip).max(1);
        let height = (dds.get_height() >> mip).max(1);
        let depth = (dds.get_depth() >> mip).max(1);
        let slice_len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| color_eyre::eyre::eyre!("L8 DDS volume size overflow"))?;
        let mut encoded_slices = Vec::with_capacity(depth as usize);
        for slice in 0..depth {
            let end = offset
                .checked_add(slice_len)
                .ok_or_else(|| color_eyre::eyre::eyre!("L8 DDS volume size overflow"))?;
            let luminance = dds.data.get(offset..end).ok_or_else(|| {
                color_eyre::eyre::eyre!("L8 DDS volume is truncated at mip {mip} slice {slice}")
            })?;
            let mut rgba = Vec::with_capacity(slice_len * 4);
            for &value in luminance {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
            if base_rgba.is_none() {
                base_rgba = Some(rgba.clone());
            }
            encoded_slices.push(encode_basis_ktx2(
                width,
                height,
                &rgba,
                encoding,
                false,
                etc1s_quality,
                uastc_level,
            )?);
            offset = end;
        }
        encoded_levels.push(encoded_slices);
    }
    ensure!(
        offset == dds.data.len(),
        "L8 DDS volume has {} unexpected trailing bytes",
        dds.data.len() - offset
    );
    let template = encode_basis_ktx2(
        dds.get_width(),
        dds.get_height(),
        &base_rgba.ok_or_else(|| color_eyre::eyre::eyre!("L8 DDS volume has no data"))?,
        encoding,
        mip_count > 1,
        etc1s_quality,
        uastc_level,
    )?;
    Ok((encoded_levels, template))
}

fn combine_ktx2_volume(
    template: &[u8],
    levels: &[Vec<Vec<u8>>],
    width: u32,
    height: u32,
    depth: u32,
) -> Result<Vec<u8>> {
    ensure!(
        width > 0 && height > 0 && depth > 1,
        "invalid KTX2 volume dimensions"
    );
    ensure!(!levels.is_empty(), "KTX2 volume has no levels");
    let template_reader = ktx2::Reader::new(template)
        .map_err(|error| color_eyre::eyre::eyre!("invalid KTX2 volume template: {error:?}"))?;
    let reference = template_reader.header();
    ensure!(
        reference.level_count as usize == levels.len(),
        "KTX2 volume template has {} levels, but the DDS has {}",
        reference.level_count,
        levels.len()
    );
    ensure!(
        reference.supercompression_scheme != Some(ktx2::SupercompressionScheme::BasisLZ),
        "BasisLZ volume assembly is not supported"
    );
    let level_count = levels.len();
    let level_table_end = ktx2::Header::LENGTH
        .checked_add(
            level_count
                .checked_mul(ktx2::LevelIndex::LENGTH)
                .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 volume level table overflow"))?,
        )
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 volume level table overflow"))?;
    let first_data_offset = (0..level_count)
        .map(|level| {
            let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
            let bytes: &[u8; ktx2::LevelIndex::LENGTH] = template
                [start..start + ktx2::LevelIndex::LENGTH]
                .try_into()
                .expect("fixed-size KTX2 level index");
            ktx2::LevelIndex::from_bytes(bytes).byte_offset as usize
        })
        .min()
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 volume template has no levels"))?;
    ensure!(
        first_data_offset >= level_table_end && first_data_offset <= template.len(),
        "invalid KTX2 volume metadata layout"
    );

    let mut output = template[..first_data_offset].to_vec();
    output[20..24].copy_from_slice(&width.to_le_bytes());
    output[24..28].copy_from_slice(&height.to_le_bytes());
    output[28..32].copy_from_slice(&depth.to_le_bytes());
    output[32..36].copy_from_slice(&0u32.to_le_bytes());
    output[36..40].copy_from_slice(&1u32.to_le_bytes());
    let mut indexes = Vec::with_capacity(level_count);
    for (mip, slices) in levels.iter().enumerate() {
        let expected_depth = (depth >> mip).max(1) as usize;
        ensure!(
            slices.len() == expected_depth,
            "KTX2 volume mip {mip} has {} slices, expected {expected_depth}",
            slices.len()
        );
        while !output.len().is_multiple_of(16) {
            output.push(0);
        }
        let offset = output.len() as u64;
        let mut uncompressed_length = 0u64;
        for slice in slices {
            let reader = ktx2::Reader::new(slice)
                .map_err(|error| color_eyre::eyre::eyre!("invalid KTX2 volume slice: {error:?}"))?;
            let header = reader.header();
            ensure!(
                header.level_count == 1
                    && header.pixel_depth == 0
                    && header.layer_count == 0
                    && header.face_count == 1
                    && header.format == reference.format
                    && header.supercompression_scheme == reference.supercompression_scheme,
                "KTX2 volume slice has incompatible layout"
            );
            let level = reader
                .levels()
                .next()
                .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 volume slice has no level"))?;
            output.extend_from_slice(level.data);
            uncompressed_length = uncompressed_length
                .checked_add(level.uncompressed_byte_length)
                .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 volume size overflow"))?;
        }
        indexes.push(ktx2::LevelIndex {
            byte_offset: offset,
            byte_length: output.len() as u64 - offset,
            uncompressed_byte_length: uncompressed_length,
        });
    }
    for (level, index) in indexes.iter().enumerate() {
        let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
        output[start..start + ktx2::LevelIndex::LENGTH].copy_from_slice(&index.as_bytes());
    }
    Ok(output)
}

fn combine_ktx2_cubemap_faces(faces: &[Vec<u8>]) -> Result<Vec<u8>> {
    ensure!(
        faces.len() == 6,
        "a cubemap requires exactly six KTX2 faces"
    );
    let readers: Vec<_> = faces
        .iter()
        .map(|face| {
            ktx2::Reader::new(face)
                .map_err(|error| color_eyre::eyre::eyre!("invalid KTX2 cubemap face: {error:?}"))
        })
        .collect::<Result<_>>()?;
    let reference = readers[0].header();
    ensure!(reference.face_count == 1, "cubemap source face is not 2D");
    ensure!(
        reference.supercompression_scheme != Some(ktx2::SupercompressionScheme::BasisLZ),
        "BasisLZ cubemap assembly is not supported"
    );
    for (face, reader) in readers[1..].iter().enumerate() {
        let header = reader.header();
        ensure!(
            header.pixel_width == reference.pixel_width
                && header.pixel_height == reference.pixel_height
                && header.level_count == reference.level_count
                && header.format == reference.format
                && header.supercompression_scheme == reference.supercompression_scheme,
            "KTX2 cubemap face {} has incompatible layout: expected {reference:?}, got {header:?}",
            face + 1
        );
    }

    let level_count = reference.level_count.max(1) as usize;
    let level_table_end = ktx2::Header::LENGTH
        .checked_add(
            level_count
                .checked_mul(ktx2::LevelIndex::LENGTH)
                .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 cubemap level table overflow"))?,
        )
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 cubemap level table overflow"))?;
    let first_data_offset = (0..level_count)
        .map(|level| {
            let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
            let bytes: &[u8; ktx2::LevelIndex::LENGTH] = faces[0]
                [start..start + ktx2::LevelIndex::LENGTH]
                .try_into()
                .expect("fixed-size KTX2 level index");
            ktx2::LevelIndex::from_bytes(bytes).byte_offset as usize
        })
        .min()
        .ok_or_else(|| color_eyre::eyre::eyre!("KTX2 cubemap has no levels"))?;
    ensure!(
        first_data_offset >= level_table_end && first_data_offset <= faces[0].len(),
        "invalid KTX2 cubemap metadata layout"
    );

    let face_levels: Vec<Vec<_>> = readers
        .iter()
        .map(|reader| reader.levels().collect())
        .collect();
    let mut output = faces[0][..first_data_offset].to_vec();
    output[36..40].copy_from_slice(&6u32.to_le_bytes());
    let mut indexes = Vec::with_capacity(level_count);
    for level in 0..level_count {
        while !output.len().is_multiple_of(16) {
            output.push(0);
        }
        let offset = output.len() as u64;
        let face_length = face_levels[0][level].data.len();
        let face_uncompressed = face_levels[0][level].uncompressed_byte_length;
        for levels in &face_levels {
            ensure!(
                levels[level].data.len() == face_length
                    && levels[level].uncompressed_byte_length == face_uncompressed,
                "KTX2 cubemap face levels have incompatible sizes"
            );
            output.extend_from_slice(levels[level].data);
        }
        indexes.push(ktx2::LevelIndex {
            byte_offset: offset,
            byte_length: (face_length * 6) as u64,
            uncompressed_byte_length: face_uncompressed.saturating_mul(6),
        });
    }
    for (level, index) in indexes.iter().enumerate() {
        let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
        output[start..start + ktx2::LevelIndex::LENGTH].copy_from_slice(&index.as_bytes());
    }
    Ok(output)
}

fn encode_x8r8g8b8(
    dds: &Dds,
    encoding: TextureEncoding,
    etc1s_quality: u8,
    uastc_level: u8,
) -> Result<Vec<u8>> {
    let decoded = decode_x8r8g8b8_mips(dds)?;
    let mut levels = Vec::with_capacity(decoded.len());
    for (width, height, rgba) in &decoded {
        levels.push(encode_basis_ktx2(
            *width,
            *height,
            rgba,
            encoding,
            false,
            etc1s_quality,
            uastc_level,
        )?);
    }
    if levels.len() == 1 {
        return Ok(levels.pop().expect("one encoded X8R8G8B8 mip"));
    }
    let (width, height, rgba) = &decoded[0];
    let template = encode_basis_ktx2(
        *width,
        *height,
        rgba,
        encoding,
        true,
        etc1s_quality,
        uastc_level,
    )?;
    combine_ktx2_mip_levels(&template, &levels)
}

fn decode_x8r8g8b8_mips(dds: &Dds) -> Result<Vec<(u32, u32, Vec<u8>)>> {
    let mut offset = 0usize;
    let mut levels = Vec::with_capacity(dds.get_num_mipmap_levels() as usize);
    for mip in 0..dds.get_num_mipmap_levels() {
        let width_u32 = (dds.get_width() >> mip).max(1);
        let height_u32 = (dds.get_height() >> mip).max(1);
        let width = usize::try_from(width_u32).wrap_err("DDS width does not fit in memory")?;
        let height = usize::try_from(height_u32).wrap_err("DDS height does not fit in memory")?;
        let source_pitch = if mip == 0 {
            usize::try_from(
                dds.get_pitch()
                    .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS has no pitch"))?,
            )
            .wrap_err("DDS pitch does not fit in memory")?
        } else {
            width
                .checked_mul(4)
                .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS mip row size overflow"))?
        };
        let remaining = dds
            .data
            .get(offset..)
            .ok_or_else(|| color_eyre::eyre::eyre!("truncated X8R8G8B8 DDS before mip {mip}"))?;
        let (rgba, consumed) = decode_x8r8g8b8_level(remaining, width, height, source_pitch)
            .wrap_err_with(|| format!("invalid X8R8G8B8 DDS mip {mip}"))?;
        offset = offset
            .checked_add(consumed)
            .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS mip offset overflow"))?;
        levels.push((width_u32, height_u32, rgba));
    }
    Ok(levels)
}

#[cfg(test)]
fn decode_x8r8g8b8(dds: &Dds) -> Result<Vec<u8>> {
    decode_x8r8g8b8_mips(dds)?
        .into_iter()
        .next()
        .map(|(_, _, rgba)| rgba)
        .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS has no mip levels"))
}

fn decode_x8r8g8b8_level(
    data: &[u8],
    width: usize,
    height: usize,
    pitch: usize,
) -> Result<(Vec<u8>, usize)> {
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS row size overflow"))?;
    ensure!(
        pitch >= row_bytes,
        "X8R8G8B8 DDS pitch is smaller than a row"
    );
    let source_size = pitch
        .checked_mul(height)
        .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 DDS payload size overflow"))?;
    ensure!(data.len() >= source_size, "truncated X8R8G8B8 DDS payload");
    let output_size = row_bytes
        .checked_mul(height)
        .ok_or_else(|| color_eyre::eyre::eyre!("X8R8G8B8 RGBA size overflow"))?;
    let mut rgba = Vec::with_capacity(output_size);
    for row in data[..source_size].chunks_exact(pitch) {
        let (pixels, remainder) = row[..row_bytes].as_chunks::<4>();
        debug_assert!(remainder.is_empty());
        for pixel in pixels {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    Ok((rgba, source_size))
}

fn encode_basis_ktx2(
    width: u32,
    height: u32,
    rgba: &[u8],
    encoding: TextureEncoding,
    generate_mips: bool,
    etc1s_quality: u8,
    uastc_level: u8,
) -> Result<Vec<u8>> {
    ensure!(
        width > 0 && height > 0,
        "texture dimensions must be non-zero"
    );
    ensure!(
        rgba.len() == width as usize * height as usize * 4,
        "RGBA payload size mismatch"
    );
    BASIS_INIT.call_once(basis_universal::encoder_init);
    ensure!(etc1s_quality > 0, "ETC1S quality must be greater than zero");
    ensure!(uastc_level <= 4, "UASTC level must be between 0 and 4");
    let mut flags = FLAG_KTX2;
    if generate_mips {
        flags |= FLAG_GENERATE_MIPS_CLAMP;
    }
    // Bevy 0.19 can transcode UASTC payloads from KTX2, but its KTX2 loader
    // explicitly rejects the BasisLZ supercompression used by ETC1S. Keep the
    // offline/runtime contract compatible by emitting UASTC for every texture.
    flags |= FLAG_UASTC | u32::from(uastc_level);
    if encoding.is_srgb() {
        flags |= FLAG_SRGB;
    }
    let mut size = 0usize;
    // SAFETY: the encoder copies the complete RGBA slice during this call. The
    // returned allocation is owned by Basis and freed after copying below.
    let data =
        unsafe { opensky_basis_compress_ktx2(rgba.as_ptr(), width, height, flags, 0.0, &mut size) };
    ensure!(
        !data.is_null() && size > 0,
        "Basis Universal compression failed"
    );
    // SAFETY: a successful encoder call returns exactly `size` initialized bytes.
    let output = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec() };
    // SAFETY: `data` was allocated by basis_compress and has not been freed yet.
    unsafe { opensky_basis_free(data) };
    Ok(output)
}

fn validate_ktx2(bytes: &[u8], encoding: TextureEncoding) -> Result<()> {
    ensure!(
        bytes.starts_with(KTX2_IDENTIFIER),
        "encoder did not produce KTX2"
    );
    let reader = ktx2::Reader::new(bytes)
        .map_err(|error| color_eyre::eyre::eyre!("generated invalid KTX2: {error:?}"))?;
    ensure!(
        reader.header().pixel_width > 0,
        "KTX2 has invalid dimensions"
    );
    ensure!(
        reader.header().supercompression_scheme != Some(ktx2::SupercompressionScheme::BasisLZ),
        "runtime-incompatible BasisLZ supercompression was emitted"
    );
    ensure!(
        reader.levels().next().is_some(),
        "KTX2 contains no image levels"
    );
    let expected_transfer = if encoding.is_srgb() {
        ktx2::TransferFunction::SRGB
    } else {
        ktx2::TransferFunction::Linear
    };
    ensure!(
        reader.transfer_function() == Some(expected_transfer),
        "KTX2 transfer function does not match {encoding:?}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use basis_universal::{
        DecodeFlags, LowLevelUastcTranscoder, SliceParametersUastc, TranscoderBlockFormat,
    };
    use ddsfile::{AlphaMode, D3D10ResourceDimension, DxgiFormat, NewD3dParams, NewDxgiParams};
    use std::io::Read;

    #[test]
    fn derives_encoding_from_slot_semantics_and_resolves_shared_textures() {
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([TextureSemantic::BaseColor])).unwrap(),
            TextureEncoding::ColorSrgb
        );
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([TextureSemantic::Normal])).unwrap(),
            TextureEncoding::NormalLinear
        );
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([TextureSemantic::Height])).unwrap(),
            TextureEncoding::DataLinear
        );
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([
                TextureSemantic::BaseColor,
                TextureSemantic::Normal,
            ]))
            .unwrap(),
            TextureEncoding::NormalLinear
        );
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([
                TextureSemantic::Emissive,
                TextureSemantic::Height,
            ]))
            .unwrap(),
            TextureEncoding::DataLinear
        );
        assert_eq!(
            TextureEncoding::from_semantics(&BTreeSet::from([
                TextureSemantic::BaseColor,
                TextureSemantic::EnvironmentMask,
            ]))
            .unwrap(),
            TextureEncoding::DataLinear
        );
    }

    #[test]
    fn creates_runtime_compatible_color_ktx2() {
        let pixels = [255, 0, 0, 255].repeat(16);
        let bytes =
            encode_basis_ktx2(4, 4, &pixels, TextureEncoding::ColorSrgb, false, 192, 2).unwrap();
        validate_ktx2(&bytes, TextureEncoding::ColorSrgb).unwrap();
        let reader = ktx2::Reader::new(&bytes).unwrap();
        assert_ne!(
            reader.header().supercompression_scheme,
            Some(ktx2::SupercompressionScheme::BasisLZ)
        );
    }

    #[test]
    fn creates_uastc_normal_map_ktx2() {
        let pixels = [128, 128, 255, 255].repeat(16);
        let bytes =
            encode_basis_ktx2(4, 4, &pixels, TextureEncoding::NormalLinear, false, 192, 2).unwrap();
        validate_ktx2(&bytes, TextureEncoding::NormalLinear).unwrap();
    }

    #[test]
    fn preserves_alpha_and_normal_channels_through_uastc_round_trip() {
        let alpha_pixels: Vec<u8> = (0..16)
            .flat_map(|index| [220, 60, 20, if index < 8 { 0 } else { 255 }])
            .collect();
        let alpha_ktx = encode_basis_ktx2(
            4,
            4,
            &alpha_pixels,
            TextureEncoding::ColorSrgb,
            false,
            192,
            2,
        )
        .unwrap();
        let alpha_round_trip = decode_uastc_level(&alpha_ktx, 0);
        let (alpha_pixels, remainder) = alpha_round_trip.as_chunks::<4>();
        assert!(remainder.is_empty());
        let alpha: Vec<_> = alpha_pixels.iter().map(|pixel| pixel[3]).collect();
        assert!(alpha.iter().filter(|value| **value < 64).count() >= 4);
        assert!(alpha.iter().filter(|value| **value > 191).count() >= 4);

        // Asymmetric tangent-space vectors catch swapped/inverted X/Y channels.
        let normal_pixels: Vec<u8> = (0..16)
            .flat_map(|index| {
                if index < 8 {
                    [224, 48, 196, 255]
                } else {
                    [40, 208, 180, 255]
                }
            })
            .collect();
        let normal_ktx = encode_basis_ktx2(
            4,
            4,
            &normal_pixels,
            TextureEncoding::NormalLinear,
            false,
            192,
            2,
        )
        .unwrap();
        let normal_round_trip = decode_uastc_level(&normal_ktx, 0);
        let first = &normal_round_trip[0..4];
        let last = &normal_round_trip[60..64];
        assert!(first[0] > first[1] && last[0] < last[1]);
        assert!(first[2] > 140 && last[2] > 140);
    }

    #[test]
    fn validates_metadata_hash_dimensions_levels_and_expanded_size() {
        let pixels = [30, 80, 160, 255].repeat(64);
        let bytes =
            encode_basis_ktx2(8, 8, &pixels, TextureEncoding::ColorSrgb, true, 192, 2).unwrap();
        let metadata = inspect_ktx2(&bytes, TextureEncoding::ColorSrgb).unwrap();
        assert_eq!(
            (metadata.width, metadata.height, metadata.levels),
            (8, 8, 4)
        );
        assert_eq!(metadata.expanded_rgba_bytes, (64 + 16 + 4 + 1) * 4);
        assert_eq!(metadata.encoded_bytes, bytes.len() as u64);
        assert_eq!(metadata.sha256, crate::cache::hash_bytes(&bytes));
        assert!(!metadata.format.is_empty() && !metadata.supercompression.is_empty());
    }

    #[test]
    fn inspects_published_runtime_ktx2_without_source_semantics() {
        let pixels = [30, 80, 160, 255].repeat(16);
        let color =
            encode_basis_ktx2(4, 4, &pixels, TextureEncoding::ColorSrgb, false, 192, 2).unwrap();
        let data =
            encode_basis_ktx2(4, 4, &pixels, TextureEncoding::DataLinear, false, 192, 2).unwrap();
        assert_eq!(
            inspect_runtime_ktx2(&color).unwrap().encoding,
            TextureEncoding::ColorSrgb
        );
        assert_eq!(
            inspect_runtime_ktx2(&data).unwrap().encoding,
            TextureEncoding::DataLinear
        );
        assert!(inspect_runtime_ktx2(b"truncated").is_err());
    }

    #[test]
    fn converts_bc1_through_bc7_reachable_variants() {
        let formats = [
            DxgiFormat::BC1_UNorm,
            DxgiFormat::BC2_UNorm,
            DxgiFormat::BC3_UNorm,
            DxgiFormat::BC4_UNorm,
            DxgiFormat::BC5_UNorm,
            DxgiFormat::BC6H_UF16,
            DxgiFormat::BC7_UNorm,
        ];
        for format in formats {
            let dds = Dds::new_dxgi(NewDxgiParams {
                height: 4,
                width: 4,
                depth: None,
                format,
                mipmap_levels: None,
                array_layers: None,
                caps2: None,
                is_cubemap: false,
                resource_dimension: D3D10ResourceDimension::Texture2D,
                alpha_mode: AlphaMode::Straight,
            })
            .unwrap();
            let mut bytes = Vec::new();
            dds.write(&mut bytes).unwrap();
            TextureConverter::convert(&bytes, TextureEncoding::DataLinear)
                .unwrap_or_else(|error| panic!("failed to convert {format:?}: {error:#}"));
        }
    }

    #[test]
    fn preserves_authored_dds_mip_levels_instead_of_regenerating_them() {
        let mut dds = Dds::new_dxgi(NewDxgiParams {
            height: 4,
            width: 4,
            depth: None,
            format: DxgiFormat::R8G8B8A8_UNorm,
            mipmap_levels: Some(3),
            array_layers: None,
            caps2: None,
            is_cubemap: false,
            resource_dimension: D3D10ResourceDimension::Texture2D,
            alpha_mode: AlphaMode::Straight,
        })
        .unwrap();
        dds.data[..64].copy_from_slice(&[255, 0, 0, 255].repeat(16));
        dds.data[64..80].copy_from_slice(&[0, 255, 0, 128].repeat(4));
        dds.data[80..84].copy_from_slice(&[0, 0, 255, 32]);
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();

        let ktx = TextureConverter::convert(&bytes, TextureEncoding::ColorSrgb).unwrap();
        let metadata = inspect_ktx2(&ktx, TextureEncoding::ColorSrgb).unwrap();
        assert_eq!(metadata.levels, 3);
        let mip1 = decode_uastc_level(&ktx, 1);
        let mip2 = decode_uastc_level(&ktx, 2);
        assert!(mip1[1] > mip1[0] && mip1[1] > mip1[2]);
        assert!(mip1[3].abs_diff(128) <= 24);
        assert!(mip2[2] > mip2[0] && mip2[2] > mip2[1]);
        assert!(mip2[3].abs_diff(32) <= 24);
    }

    #[test]
    fn preserves_a_partial_authored_mip_chain() {
        let mut dds = Dds::new_dxgi(NewDxgiParams {
            height: 8,
            width: 8,
            depth: None,
            format: DxgiFormat::R8G8B8A8_UNorm,
            mipmap_levels: Some(2),
            array_layers: None,
            caps2: None,
            is_cubemap: false,
            resource_dimension: D3D10ResourceDimension::Texture2D,
            alpha_mode: AlphaMode::Straight,
        })
        .unwrap();
        dds.data[..256].copy_from_slice(&[255, 0, 0, 255].repeat(64));
        dds.data[256..320].copy_from_slice(&[0, 255, 0, 255].repeat(16));
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();

        let ktx = TextureConverter::convert(&bytes, TextureEncoding::ColorSrgb).unwrap();
        assert_eq!(
            inspect_ktx2(&ktx, TextureEncoding::ColorSrgb)
                .unwrap()
                .levels,
            2
        );
        let authored_mip = decode_uastc_level(&ktx, 1);
        assert!(authored_mip[1] > authored_mip[0] && authored_mip[1] > authored_mip[2]);
    }

    #[test]
    fn generates_mipmap_chain_for_mipped_source() {
        let pixels = [64, 128, 192, 255].repeat(64);
        let bytes =
            encode_basis_ktx2(8, 8, &pixels, TextureEncoding::ColorSrgb, true, 192, 2).unwrap();
        let reader = ktx2::Reader::new(&bytes).unwrap();
        assert_eq!(reader.header().level_count, 4);
        assert_eq!(reader.levels().count(), 4);
    }

    #[test]
    fn assembles_six_faces_into_a_cubemap_ktx2() {
        let faces: Vec<_> = (0..6)
            .map(|face| {
                let pixels = [face * 20, 64, 128, 255].repeat(16);
                encode_basis_ktx2(4, 4, &pixels, TextureEncoding::ColorSrgb, true, 192, 2).unwrap()
            })
            .collect();

        let cubemap = combine_ktx2_cubemap_faces(&faces).unwrap();
        let reader = ktx2::Reader::new(&cubemap).unwrap();
        assert_eq!(reader.header().face_count, 6);
        assert_eq!(reader.header().layer_count, 0);
        assert_eq!(reader.header().level_count, 3);
        for (combined, single) in reader
            .levels()
            .zip(ktx2::Reader::new(&faces[0]).unwrap().levels())
        {
            assert_eq!(combined.data.len(), single.data.len() * 6);
            assert_eq!(
                combined.uncompressed_byte_length,
                single.uncompressed_byte_length * 6
            );
        }
    }

    #[test]
    fn assembles_depth_slices_into_a_volume_ktx2() {
        let template = encode_basis_ktx2(
            4,
            4,
            &[64, 64, 64, 255].repeat(16),
            TextureEncoding::DataLinear,
            true,
            192,
            2,
        )
        .unwrap();
        let levels: Vec<Vec<Vec<u8>>> = [(4, 4, 4), (2, 2, 2), (1, 1, 1)]
            .into_iter()
            .map(|(width, height, depth)| {
                (0..depth)
                    .map(|slice| {
                        encode_basis_ktx2(
                            width,
                            height,
                            &[slice as u8 * 20, 80, 120, 255].repeat((width * height) as usize),
                            TextureEncoding::DataLinear,
                            false,
                            192,
                            2,
                        )
                        .unwrap()
                    })
                    .collect()
            })
            .collect();

        let volume = combine_ktx2_volume(&template, &levels, 4, 4, 4).unwrap();
        let reader = ktx2::Reader::new(&volume).unwrap();
        assert_eq!(reader.header().pixel_width, 4);
        assert_eq!(reader.header().pixel_height, 4);
        assert_eq!(reader.header().pixel_depth, 4);
        assert_eq!(reader.header().level_count, 3);
        assert_eq!(reader.levels().count(), 3);
    }

    #[test]
    #[ignore = "requires OPENSKYRIM_DDS_FIXTURE with a locally installed cubemap"]
    fn converts_installed_cubemap_fixture() {
        let path = std::env::var_os("OPENSKYRIM_DDS_FIXTURE")
            .map(std::path::PathBuf::from)
            .expect("set OPENSKYRIM_DDS_FIXTURE to a cubemap DDS");
        let bytes = std::fs::read(&path).unwrap();

        let converted = TextureConverter::convert(&bytes, TextureEncoding::ColorSrgb)
            .unwrap_or_else(|error| panic!("failed to convert {}: {error:#}", path.display()));
        let reader = ktx2::Reader::new(&converted).unwrap();
        assert_eq!(reader.header().face_count, 6);
    }

    #[test]
    #[ignore = "requires OPENSKYRIM_COLOR_DDS_FIXTURE with a locally installed color DDS"]
    fn converts_installed_color_fixture() {
        convert_installed_2d_fixture("OPENSKYRIM_COLOR_DDS_FIXTURE", TextureEncoding::ColorSrgb);
    }

    #[test]
    #[ignore = "requires OPENSKYRIM_NORMAL_DDS_FIXTURE with a locally installed normal DDS"]
    fn converts_installed_normal_fixture() {
        convert_installed_2d_fixture(
            "OPENSKYRIM_NORMAL_DDS_FIXTURE",
            TextureEncoding::NormalLinear,
        );
    }

    #[test]
    #[ignore = "requires OPENSKYRIM_VOLUME_DDS_FIXTURE with a locally installed volume DDS"]
    fn converts_installed_volume_fixture() {
        let path = std::env::var_os("OPENSKYRIM_VOLUME_DDS_FIXTURE")
            .map(std::path::PathBuf::from)
            .expect("set OPENSKYRIM_VOLUME_DDS_FIXTURE to a volume DDS");
        let bytes = std::fs::read(&path).unwrap();

        let converted = TextureConverter::convert(&bytes, TextureEncoding::DataLinear)
            .unwrap_or_else(|error| panic!("failed to convert {}: {error:#}", path.display()));
        let reader = ktx2::Reader::new(&converted).unwrap();
        assert_eq!(reader.header().pixel_depth, 128);
        assert_eq!(reader.header().level_count, 8);
    }

    #[test]
    fn decodes_x8r8g8b8_as_opaque_rgba() {
        let mut dds = x8r8g8b8_fixture();
        dds.data.copy_from_slice(&[3, 2, 1, 99, 30, 20, 10, 88]);

        assert_eq!(
            decode_x8r8g8b8(&dds).unwrap(),
            [1, 2, 3, 255, 10, 20, 30, 255]
        );
    }

    #[test]
    fn rejects_truncated_x8r8g8b8() {
        let mut dds = x8r8g8b8_fixture();
        dds.data.pop();

        let error = decode_x8r8g8b8(&dds).unwrap_err();
        let error_chain = format!("{error:#}");
        assert!(error_chain.contains("truncated"), "{error_chain}");
    }

    #[test]
    fn converts_x8r8g8b8_to_runtime_compatible_ktx2() {
        let mut dds = x8r8g8b8_fixture();
        dds.data.copy_from_slice(&[3, 2, 1, 0, 30, 20, 10, 0]);
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();

        let ktx2 = TextureConverter::convert(&bytes, TextureEncoding::ColorSrgb).unwrap();
        validate_ktx2(&ktx2, TextureEncoding::ColorSrgb).unwrap();
    }

    #[test]
    fn preserves_mipped_x8r8g8b8_chain() {
        let mut dds = Dds::new_d3d(NewD3dParams {
            height: 4,
            width: 4,
            depth: None,
            format: D3DFormat::X8R8G8B8,
            mipmap_levels: Some(3),
            caps2: None,
        })
        .unwrap();
        for (index, byte) in dds.data.iter_mut().enumerate() {
            *byte = index as u8;
        }
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();

        let ktx2 = TextureConverter::convert(&bytes, TextureEncoding::DataLinear).unwrap();
        let metadata = inspect_ktx2(&ktx2, TextureEncoding::DataLinear).unwrap();
        assert_eq!(metadata.levels, 3);
        assert_eq!((metadata.width, metadata.height), (4, 4));
    }

    #[test]
    fn atomically_replaces_an_existing_published_texture() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("fixture.dds");
        let output = directory.path().join("fixture.ktx2");
        let mut dds = x8r8g8b8_fixture();
        dds.data.copy_from_slice(&[3, 2, 1, 0, 30, 20, 10, 0]);
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();
        fs::write(&input, bytes).unwrap();

        TextureConverter::convert_dds_to_ktx2(&input, &output, TextureEncoding::ColorSrgb).unwrap();
        let first = fs::read(&output).unwrap();
        TextureConverter::convert_dds_to_ktx2(&input, &output, TextureEncoding::DataLinear)
            .unwrap();
        let second = fs::read(&output).unwrap();
        assert_ne!(first, second);
        inspect_ktx2(&second, TextureEncoding::DataLinear).unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn generated_dds_never_panic_under_truncation_or_mutation() {
        let mut rng = dummy_content::rng::Rng::new(9);
        let specs = [
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 8, 8)
                .with_mip_levels(4),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc5Unorm, 8, 8),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc7Unorm, 8, 8),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::X8R8G8B8, 4, 4),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 4, 4).as_cubemap(),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 4, 4).with_depth(4),
        ];
        for spec in specs {
            let bytes = dummy_content::dds::generate(&spec, &mut rng).unwrap();
            let mut lengths: Vec<usize> = (0..bytes.len()).step_by(31).collect();
            lengths.extend(0..bytes.len().min(256));
            for length in lengths {
                let result = std::panic::catch_unwind(|| {
                    TextureConverter::convert(&bytes[..length], TextureEncoding::ColorSrgb)
                });
                assert!(result.is_ok(), "DDS converter panicked at length {length}");
            }
            for _ in 0..128 {
                let mut mutated = bytes.clone();
                let index = rng.next_u64() as usize % mutated.len();
                mutated[index] ^= 0xff;
                let result = std::panic::catch_unwind(|| {
                    TextureConverter::convert(&mutated, TextureEncoding::NormalLinear)
                });
                assert!(
                    result.is_ok(),
                    "DDS converter panicked on mutation at {index}"
                );
            }
        }
    }

    #[test]
    fn zstd_supercompression_keeps_every_level_byte_identical() {
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 64, 64)
                .with_mip_levels(4),
            &mut dummy_content::rng::Rng::new(4),
        )
        .unwrap();

        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::ColorSrgb, 192, 2).unwrap();
        let published = supercompress_ktx2(&raw).unwrap();

        let raw_reader = ktx2::Reader::new(&raw).unwrap();
        let reader = ktx2::Reader::new(&published).unwrap();
        let header = reader.header();
        assert_eq!(
            header.supercompression_scheme,
            Some(ktx2::SupercompressionScheme::Zstandard)
        );
        assert_eq!(header.level_count, raw_reader.header().level_count);
        assert_eq!(header.pixel_width, raw_reader.header().pixel_width);
        assert_eq!(header.pixel_height, raw_reader.header().pixel_height);
        assert_eq!(reader.transfer_function(), raw_reader.transfer_function());

        let raw_levels: Vec<&[u8]> = raw_reader.levels().map(|level| level.data).collect();
        let decoded = decode_ktx2_levels(&published);
        assert_eq!(decoded.len(), raw_levels.len());
        for (mip, (decoded, raw)) in decoded.iter().zip(&raw_levels).enumerate() {
            assert_eq!(decoded, *raw, "mip {mip} changed under supercompression");
        }
        for (mip, (level, raw)) in reader.levels().zip(&raw_levels).enumerate() {
            assert_eq!(
                level.uncompressed_byte_length,
                raw.len() as u64,
                "mip {mip} does not record its uncompressed length"
            );
        }

        // Only the level index and the scheme field may differ: the header around them, the DFD
        // and the key/value data are copied through byte for byte.
        let table_end =
            ktx2::Header::LENGTH + header.level_count as usize * ktx2::LevelIndex::LENGTH;
        let index = header.index;
        let metadata_end = (index.dfd_byte_offset + index.dfd_byte_length)
            .max(index.kvd_byte_offset + index.kvd_byte_length) as usize;
        assert_eq!(raw[..44], published[..44]);
        assert_eq!(&published[44..48], 2u32.to_le_bytes());
        assert_eq!(
            raw[48..ktx2::Header::LENGTH],
            published[48..ktx2::Header::LENGTH]
        );
        assert_eq!(
            raw[table_end..metadata_end],
            published[table_end..metadata_end]
        );
        assert!(metadata_end > table_end, "the container carries no DFD/KVD");
    }

    #[test]
    fn publishes_zstd_supercompressed_cubemaps_and_volumes() {
        for spec in [
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 64, 64)
                .with_mip_levels(4),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 32, 32)
                .as_cubemap(),
            dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 8, 8).with_depth(4),
        ] {
            let dds =
                dummy_content::dds::generate(&spec, &mut dummy_content::rng::Rng::new(6)).unwrap();
            let published =
                TextureConverter::convert_with_options(&dds, TextureEncoding::DataLinear, 192, 2)
                    .unwrap();

            inspect_ktx2(&published, TextureEncoding::DataLinear).unwrap();
            assert!(is_zstd_supercompressed(&published));
            let reader = ktx2::Reader::new(&published).unwrap();
            let levels = decode_ktx2_levels(&published);
            assert_eq!(levels.len(), reader.header().level_count as usize);
            assert!(
                levels.iter().all(|level| !level.is_empty()),
                "a level lost its payload"
            );
            assert_eq!(reader.header().face_count, if spec.cubemap { 6 } else { 1 });
            assert_eq!(reader.header().pixel_depth, spec.depth.unwrap_or(0));
        }
    }

    #[test]
    fn reuses_a_raw_container_by_supercompressing_its_levels() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("legacy.ktx2");
        let output = directory.path().join("published.ktx2");
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 32, 32)
                .with_mip_levels(3),
            &mut dummy_content::rng::Rng::new(5),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::DataLinear, 192, 2).unwrap();
        fs::write(&source, &raw).unwrap();
        assert!(!file_is_zstd_supercompressed(&source));

        supercompress_ktx2_file(&source, &output).unwrap();

        let published = fs::read(&output).unwrap();
        assert!(file_is_zstd_supercompressed(&output));
        assert_eq!(decode_ktx2_levels(&published), decode_ktx2_levels(&raw));
        assert_eq!(fs::read(&source).unwrap(), raw, "the source was rewritten");
    }

    #[test]
    fn supercompresses_a_staged_texture_in_place() {
        let directory = tempfile::tempdir().unwrap();
        let staged = directory.path().join("rock.ktx2");
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 16, 16),
            &mut dummy_content::rng::Rng::new(8),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::DataLinear, 192, 2).unwrap();
        fs::write(&staged, &raw).unwrap();

        supercompress_ktx2_file(&staged, &staged).unwrap();

        let published = fs::read(&staged).unwrap();
        assert!(file_is_zstd_supercompressed(&staged));
        assert_eq!(decode_ktx2_levels(&published), decode_ktx2_levels(&raw));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn supercompression_keeps_the_order_the_levels_have_in_the_file() {
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 32, 32)
                .with_mip_levels(3),
            &mut dummy_content::rng::Rng::new(9),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::DataLinear, 192, 2).unwrap();
        // A container whose payloads are not stored in mip order: the rewrite follows the file, not
        // the mip number.
        let indexes = level_indexes(&raw);
        let first_data = indexes
            .iter()
            .map(|index| index.byte_offset as usize)
            .min()
            .unwrap();
        let mut permuted = raw[..first_data].to_vec();
        let mut offsets = vec![0u64; indexes.len()];
        for level in (0..indexes.len()).rev() {
            offsets[level] = permuted.len() as u64;
            let start = indexes[level].byte_offset as usize;
            permuted.extend_from_slice(&raw[start..start + indexes[level].byte_length as usize]);
        }
        for (level, offset) in offsets.iter().enumerate() {
            let index = ktx2::LevelIndex {
                byte_offset: *offset,
                byte_length: indexes[level].byte_length,
                uncompressed_byte_length: indexes[level].uncompressed_byte_length,
            };
            let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
            permuted[start..start + ktx2::LevelIndex::LENGTH].copy_from_slice(&index.as_bytes());
        }

        let published = supercompress_ktx2(&permuted).unwrap();

        let placement = |bytes: &[u8]| {
            let mut placed: Vec<(u64, usize, u64)> = level_indexes(bytes)
                .iter()
                .enumerate()
                .map(|(mip, index)| (index.byte_offset, mip, index.byte_length))
                .collect();
            placed.sort_unstable();
            placed
        };
        let placed = placement(&published);
        assert_eq!(
            placed.iter().map(|(_, mip, _)| *mip).collect::<Vec<_>>(),
            placement(&permuted)
                .iter()
                .map(|(_, mip, _)| *mip)
                .collect::<Vec<_>>(),
            "the levels were reordered"
        );
        assert_eq!(
            placed.iter().map(|(_, mip, _)| *mip).collect::<Vec<_>>(),
            (0..indexes.len()).rev().collect::<Vec<_>>(),
            "the fixture no longer stores its levels in reverse mip order"
        );
        for pair in placed.windows(2) {
            assert_eq!(
                pair[0].0 + pair[0].2,
                pair[1].0,
                "supercompressed levels carry padding"
            );
        }
        assert_eq!(
            crate::texture::decode_ktx2_levels(&published),
            crate::texture::decode_ktx2_levels(&permuted)
        );
    }

    #[test]
    fn rejects_containers_that_already_carry_supercompression_data() {
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 8, 8),
            &mut dummy_content::rng::Rng::new(13),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::DataLinear, 192, 2).unwrap();
        let published = supercompress_ktx2(&raw).unwrap();

        let already = supercompress_ktx2(&published).unwrap_err();
        assert!(
            format!("{already:#}").contains("already supercompressed"),
            "{already:#}"
        );

        // BasisLZ keeps its codebooks and image descriptors in supercompression global data.
        let mut basis_lz = raw.clone();
        basis_lz[44..48].copy_from_slice(&1u32.to_le_bytes());
        basis_lz[64..72].copy_from_slice(&432u64.to_le_bytes());
        basis_lz[72..80].copy_from_slice(&16u64.to_le_bytes());
        let error = supercompress_ktx2(&basis_lz).unwrap_err();
        assert!(
            format!("{error:#}").contains("already supercompressed"),
            "{error:#}"
        );

        // A declared offset with no length is still global data this rewrite cannot carry.
        let mut offset_only = raw.clone();
        offset_only[64..72].copy_from_slice(&432u64.to_le_bytes());
        let error = supercompress_ktx2(&offset_only).unwrap_err();
        assert!(format!("{error:#}").contains("global data"), "{error:#}");
    }

    #[test]
    fn the_file_path_reports_a_damaged_container_instead_of_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("legacy.ktx2");
        let output = directory.path().join("published.ktx2");
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 8, 8),
            &mut dummy_content::rng::Rng::new(14),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::DataLinear, 192, 2).unwrap();

        // Find a descriptor corruption the ktx2 crate's parser panics on: the file path has to
        // report it, not abort, and the asset is then re-encoded from its DDS.
        let dfd_start = u32::from_le_bytes(raw[48..52].try_into().unwrap()) as usize;
        let dfd_end = dfd_start + u32::from_le_bytes(raw[52..56].try_into().unwrap()) as usize;
        let mut damaged = None;
        for index in dfd_start..dfd_end {
            let mut candidate = raw.clone();
            candidate[index] ^= 0xff;
            if std::panic::catch_unwind(|| ktx2::Reader::new(&candidate)).is_err() {
                damaged = Some(candidate);
                break;
            }
        }
        let damaged = damaged.expect("no descriptor corruption panicked the ktx2 parser");

        fs::write(&source, &damaged).unwrap();
        let rejected = std::panic::catch_unwind(|| supercompress_ktx2_file(&source, &output));
        assert!(rejected.is_ok(), "the file path panicked");
        assert!(rejected.unwrap().is_err(), "a damaged descriptor passed");
        assert!(!output.exists(), "a damaged container was published");

        // A truncated container is reported the same way.
        fs::write(&source, &raw[..raw.len() / 2]).unwrap();
        assert!(supercompress_ktx2_file(&source, &output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn supercompression_never_panics_under_truncation_or_mutation() {
        let dds = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::Bc1Unorm, 8, 8)
                .with_mip_levels(2),
            &mut dummy_content::rng::Rng::new(11),
        )
        .unwrap();
        let raw =
            TextureConverter::encode_container(&dds, TextureEncoding::ColorSrgb, 192, 2).unwrap();
        let mut rng = dummy_content::rng::Rng::new(12);
        let mut lengths: Vec<usize> = (0..raw.len()).step_by(7).collect();
        lengths.extend(0..raw.len().min(256));
        for length in lengths {
            assert!(
                std::panic::catch_unwind(|| supercompress_ktx2(&raw[..length])).is_ok(),
                "supercompression panicked at length {length}"
            );
        }
        for _ in 0..256 {
            let mut mutated = raw.clone();
            let index = rng.next_u64() as usize % mutated.len();
            mutated[index] ^= 0xff;
            assert!(
                std::panic::catch_unwind(|| supercompress_ktx2(&mutated)).is_ok(),
                "supercompression panicked on mutation at {index}"
            );
        }
        // A damaged level table can describe an empty level; it still has to produce a frame.
        let mut empty = raw.clone();
        empty[88..96].copy_from_slice(&0u64.to_le_bytes());
        assert!(std::panic::catch_unwind(|| supercompress_ktx2(&empty)).is_ok());
        assert!(supercompress_ktx2(&empty).is_ok());
    }

    fn level_indexes(bytes: &[u8]) -> Vec<ktx2::LevelIndex> {
        let level_count = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        (0..level_count)
            .map(|level| {
                let start = ktx2::Header::LENGTH + level * ktx2::LevelIndex::LENGTH;
                ktx2::LevelIndex::from_bytes(
                    <&[u8; ktx2::LevelIndex::LENGTH]>::try_from(
                        &bytes[start..start + ktx2::LevelIndex::LENGTH],
                    )
                    .unwrap(),
                )
            })
            .collect()
    }

    fn x8r8g8b8_fixture() -> Dds {
        let bytes = dummy_content::dds::generate(
            &dummy_content::dds::Spec::new(dummy_content::dds::Format::X8R8G8B8, 2, 1),
            &mut dummy_content::rng::Rng::new(0),
        )
        .unwrap();
        Dds::read(bytes.as_slice()).unwrap()
    }

    fn convert_installed_2d_fixture(variable: &str, encoding: TextureEncoding) {
        let path = std::env::var_os(variable)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| panic!("set {variable} to an installed DDS"));
        let dds_bytes = std::fs::read(&path).unwrap();
        let dds = Dds::read(Cursor::new(&dds_bytes)).unwrap();
        let converted = TextureConverter::convert(&dds_bytes, encoding)
            .unwrap_or_else(|error| panic!("failed to convert {}: {error:#}", path.display()));
        let metadata = inspect_ktx2(&converted, encoding).unwrap();
        assert_eq!(metadata.width, dds.get_width());
        assert_eq!(metadata.height, dds.get_height());
        assert_eq!(metadata.levels, dds.get_num_mipmap_levels());
        assert_eq!(metadata.faces, 1);
    }

    fn decode_uastc_level(bytes: &[u8], mip: usize) -> Vec<u8> {
        let reader = ktx2::Reader::new(bytes).unwrap();
        let header = reader.header();
        let level = reader.levels().nth(mip).unwrap();
        let mut uastc = Vec::new();
        match header.supercompression_scheme {
            Some(ktx2::SupercompressionScheme::Zstandard) => {
                let mut cursor = Cursor::new(level.data);
                let mut decoder = ruzstd::decoding::StreamingDecoder::new(&mut cursor).unwrap();
                decoder.read_to_end(&mut uastc).unwrap();
            }
            Some(ktx2::SupercompressionScheme::ZLIB) => {
                let mut decoder = flate2::bufread::ZlibDecoder::new(level.data);
                decoder.read_to_end(&mut uastc).unwrap();
            }
            None => uastc.extend_from_slice(level.data),
            other => panic!("unsupported test supercompression {other:?}"),
        }
        let width = (header.pixel_width >> mip).max(1);
        let height = (header.pixel_height.max(1) >> mip).max(1);
        let bc7 = LowLevelUastcTranscoder::new()
            .transcode_slice(
                &uastc,
                SliceParametersUastc {
                    num_blocks_x: width.div_ceil(4),
                    num_blocks_y: height.div_ceil(4),
                    has_alpha: true,
                    original_width: width,
                    original_height: height,
                },
                DecodeFlags::HIGH_QUALITY,
                TranscoderBlockFormat::BC7,
            )
            .unwrap();
        let mut dds = Dds::new_dxgi(NewDxgiParams {
            height,
            width,
            depth: None,
            format: DxgiFormat::BC7_UNorm,
            mipmap_levels: None,
            array_layers: None,
            caps2: None,
            is_cubemap: false,
            resource_dimension: D3D10ResourceDimension::Texture2D,
            alpha_mode: AlphaMode::Straight,
        })
        .unwrap();
        dds.data.copy_from_slice(&bc7);
        image_dds::SurfaceRgba8::decode_dds(&dds)
            .unwrap()
            .get_image(0, 0, 0)
            .unwrap()
            .into_raw()
    }
}
