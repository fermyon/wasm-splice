use std::{
    fmt,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use wasm_encoder::{Encode, SectionId};
use wasmparser::{BinaryReader, CustomSectionReader, Parser, Payload};

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

pub fn transform_custom_sections<Output: Write>(
    input_path: impl AsRef<Path>,
    output: &mut Output,
    will_transform: impl Fn(&str) -> bool,
    transform: impl Fn(CustomSectionReader, &mut Output) -> Result<()>,
) -> Result<()> {
    // Input file
    let input_path = input_path.as_ref();
    let input =
        std::fs::read(input_path).with_context(|| format!("Couldn't read input {input_path:?}"))?;

    let mut consumed = 0;
    for payload in Parser::new(0).parse_all(&input) {
        if let Payload::CustomSection(reader) = payload? {
            if will_transform(reader.name()) {
                // Copy up to the beginning of this section to output
                // FIXME: This is terrible; probably shouldn't use Parser::parse_all
                let section_size_len =
                    leb128::write::unsigned(&mut std::io::sink(), reader.range().len() as u64)
                        .unwrap();
                let section_start = reader.range().start - 1 - section_size_len;
                output.write_all(&input[consumed..section_start])?;
                consumed = reader.range().end;

                // Run transform
                transform(reader, output)?;
            }
        };
    }

    // Write the remainder to output
    output.write_all(&input[consumed..])?;

    Ok(())
}

pub struct ExternalSection<'a> {
    pub section_id: u8,
    pub prefix: &'a [u8],
    pub external_size: u32,
    pub digest_algo: &'a str,
    pub digest_data: &'a [u8],
}

impl<'a> ExternalSection<'a> {
    pub const CUSTOM_SECTION_NAME: &str = "@external-section";

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(512);

        data.push(self.section_id);
        self.prefix.encode(&mut data);
        self.external_size.encode(&mut data);
        self.digest_algo.encode(&mut data);
        self.digest_data.encode(&mut data);

        data
    }

    pub fn write_custom_section(&self, mut writer: impl Write) -> std::io::Result<usize> {
        let mut name_bytes = vec![];
        Self::CUSTOM_SECTION_NAME.encode(&mut name_bytes);
        let section_data = self.to_bytes();
        let payload_size = name_bytes.len() + section_data.len();

        let header_len = write_section_header(&mut writer, SectionId::Custom as u8, payload_size)?;
        writer.write_all(&name_bytes)?;
        writer.write_all(&section_data)?;

        Ok(header_len + payload_size)
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
            section_id,
            prefix,
            external_size,
            digest_algo,
            digest_data,
        })
    }

    pub fn from_custom_section(section: CustomSectionReader) -> Result<ExternalSection> {
        ensure!(
            section.name() == Self::CUSTOM_SECTION_NAME,
            "not an external section!"
        );
        Self::from_bytes(section.data())
    }
}

impl<'a> fmt::Debug for ExternalSection<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalSection")
            .field("section_id", &self.section_id)
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
