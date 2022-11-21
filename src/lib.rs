use std::{
    fmt,
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use wasm_encoder::Encode;
use wasmparser::{BinaryReader, Parser, Payload};

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
    input_path: impl AsRef<Path>,
    output: &mut Output,
    // Return Some(section_data_range) iff section should be transformed
    will_transform: impl Fn(&Payload) -> Option<Range<usize>>,
    transform: impl Fn(Payload, &mut Output) -> Result<()>,
) -> Result<()> {
    // Input file
    let input_path = input_path.as_ref();
    let input =
        std::fs::read(input_path).with_context(|| format!("Couldn't read input {input_path:?}"))?;

    let mut consumed = 0;
    for payload_res in Parser::new(0).parse_all(&input) {
        let payload = payload_res?;
        if let Some(data_range) = will_transform(&payload) {
            // Copy up to the beginning of this section to output
            // FIXME: This is terrible; probably shouldn't use Parser::parse_all
            let section_size_len =
                leb128::write::unsigned(&mut std::io::sink(), data_range.len() as u64).unwrap();
            let section_start = data_range.start - 1 - section_size_len;
            output.write_all(&input[consumed..section_start])?;
            consumed = data_range.end;

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
