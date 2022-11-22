use std::{fmt, io::Write, path::PathBuf};

use anyhow::{ensure, Context, Result};
use wasm_encoder::Encode;
use wasmparser::{BinaryReader, Parser, Payload};

// This is effectively a "uses external sections" feature flag, using the
// "layer" field described here:
// https://github.com/WebAssembly/component-model/blob/ded219eff2f3ac8aabd34137fbda8eef18ab583b/design/mvp/Binary.md
pub const EXTERNAL_SECTION_LAYER_BIT: u32 = 0x00020000;

pub struct SpliceConfig {
    external_section_dir: PathBuf,
    external_section_extension: String,
}

impl SpliceConfig {
    pub fn external_section_path(&self, digest: impl AsRef<[u8]>) -> Result<PathBuf> {
        ensure!(!digest.as_ref().is_empty(), "digest cannot be empty");
        Ok(self
            .external_section_dir
            .join(hex::encode(digest))
            .with_extension(&self.external_section_extension))
    }
}

impl Default for SpliceConfig {
    fn default() -> Self {
        Self {
            external_section_dir: Default::default(),
            external_section_extension: "ext".to_string(),
        }
    }
}

pub fn transform_sections<Output: Write>(
    input: &[u8],
    output: &mut Output,
    // Return Some(section_start_offset) iff section should be transformed
    will_transform: impl Fn(&Payload) -> bool,
    transform: impl Fn(Payload, &mut Output) -> Result<()>,
) -> Result<()> {
    // Input file
    let mut consumed = 0;
    for payload_res in Parser::new(0).parse_all(input) {
        let payload = payload_res?;
        if will_transform(&payload) {
            let payload_range = if let Some((_, mut range)) = payload.as_section() {
                // The Range returned by Payload::as_section does not include
                // the section ID or (variable width) size, so calculate them.
                // FIXME: kinda terrible; patch wasmparser or don't use parse_all?
                let section_size_len =
                    leb128::write::unsigned(&mut std::io::sink(), range.len() as u64).unwrap();
                // From start of section data, subtract lengths of ID and section size
                range.start -= 1 + section_size_len;
                range
            } else if let Payload::Version { range, .. } = &payload {
                // Version range *does* cover the entire header
                range.clone()
            } else {
                unimplemented!("transform_sections cannot transform {payload:?}");
            };

            // Copy up to the beginning of this section to output
            output.write_all(&input[consumed..payload_range.start])?;
            consumed = payload_range.end;

            // Run transform
            transform(payload, output)?;
        }
    }

    // Write the remainder to output
    output.write_all(&input[consumed..])?;

    Ok(())
}

pub struct ExternalSection<'a> {
    pub external_section_id: u8,
    pub prefix: &'a [u8],
    pub external_size: u32,
    pub digest_algo: &'a str,
    pub digest_data: &'a [u8],
}

impl<'a> ExternalSection<'a> {
    pub const SECTION_ID: u8 = 0x5E; // 5ection External

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(512);

        data.push(self.external_section_id);
        self.prefix.encode(&mut data);
        self.external_size.encode(&mut data);
        self.digest_algo.encode(&mut data);
        self.digest_data.encode(&mut data);

        data
    }

    pub fn write_section(&self, mut writer: impl Write) -> std::io::Result<usize> {
        let section_data = self.to_bytes();
        let header_len = write_section_header(&mut writer, Self::SECTION_ID, section_data.len())?;
        writer.write_all(&section_data)?;

        Ok(header_len + section_data.len())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<ExternalSection> {
        let mut reader = BinaryReader::new(bytes);

        let section_id = reader.read_u8().context("section_id")?;
        let prefix = read_var_bytes(&mut reader).context("prefix")?;
        let external_size = reader.read_var_u32().context("external_size")?;
        let digest_algo = reader.read_string().context("digest_algo")?;
        let digest_data = read_var_bytes(&mut reader).context("digest_data")?;

        ensure!(reader.eof(), "unexpected trailing data");

        Ok(ExternalSection {
            external_section_id: section_id,
            prefix,
            external_size,
            digest_algo,
            digest_data,
        })
    }
}

impl<'a> fmt::Debug for ExternalSection<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalSection")
            .field("section_id", &self.external_section_id)
            .field("prefix", &self.prefix.escape_ascii().to_string())
            .field("external_size", &self.external_size)
            .field("digest_algo", &self.digest_algo)
            .field("digest_data", &hex::encode(self.digest_data))
            .finish()
    }
}

pub fn write_section_header(
    mut writer: impl Write,
    section_id: u8,
    payload_size: usize,
) -> std::io::Result<usize> {
    let mut section_header = vec![section_id];
    payload_size.encode(&mut section_header);
    writer.write_all(&section_header)?;
    Ok(section_header.len())
}

fn read_var_bytes<'a>(reader: &mut BinaryReader<'a>) -> Result<&'a [u8]> {
    let len = reader.read_var_u32()?;
    Ok(reader.read_bytes(len as usize)?)
}
