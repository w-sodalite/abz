use crate::app::Message;
use crate::format::Format;
use crate::status::Status;
use crate::widget::icon::icon;
use anyhow::anyhow;
use flate2::Compression;
use human_bytes::human_bytes;
use iced::widget::text::Wrapping;
use iced::widget::{button, container, horizontal_space, hover, progress_bar, row, text};
use iced::{Alignment, Border, Element, Length, Theme};
use lucide_rs::Lucide;
use std::fs::{File, metadata};
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use tar::{EntryType, Header};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct Archive {
    pub path: PathBuf,
    pub format: Format,
    pub size: u64,
    pub status: Status,
}

impl Archive {
    pub fn parse(path: impl AsRef<Path>) -> std::io::Result<Option<Self>> {
        let path = path.as_ref();
        let metadata = metadata(&path)?;
        match metadata.is_file() {
            true => match path.file_name().and_then(|s| s.to_str()) {
                Some(file_name) => match Format::parse(file_name) {
                    Some(format) => Ok(Some(Archive {
                        path: path.to_path_buf(),
                        format,
                        size: metadata.len(),
                        status: Default::default(),
                    })),
                    None => Ok(None),
                },
                None => Ok(None),
            },
            false => Ok(None),
        }
    }

    pub fn view(&self, index: usize) -> Element<'_, Message> {
        let base = container(
            row![
                iced::widget::column![
                    text(self.path.display().to_string()),
                    row![
                        text(human_bytes(self.size as f64))
                            .width(Length::Fixed(60.))
                            .wrapping(Wrapping::WordOrGlyph),
                        text(self.status.to_string())
                            .color(self.status.color())
                            .width(Length::Fill)
                    ]
                    .align_y(Alignment::Center)
                    .spacing(5),
                ]
                .spacing(5),
                match self.status {
                    Status::Processing(ratio) => Element::from(
                        row![
                            progress_bar(0.0..=1.0, ratio)
                                .length(Length::Fixed(100.))
                                .girth(Length::Fixed(5.)),
                            text(format!("{:.02}%", ratio * 100.)).width(Length::Fixed(30.))
                        ]
                        .align_y(Alignment::Center)
                        .spacing(5),
                    ),
                    Status::Success => Element::from(
                        row![
                            progress_bar(0.0..=1.0, 1.0)
                                .length(Length::Fixed(100.))
                                .girth(Length::Fixed(5.))
                                .style(progress_bar::success),
                            text("100%").width(Length::Fixed(30.)).style(text::success)
                        ]
                        .align_y(Alignment::Center)
                        .spacing(5),
                    ),
                    _ => horizontal_space().width(Length::Fixed(130.)).into(),
                },
                horizontal_space().width(Length::Fixed(40.))
            ]
            .spacing(20)
            .align_y(Alignment::Center),
        )
        .padding(5)
        .width(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Center)
        .style(|theme: &Theme| {
            container::transparent(theme).border(
                Border::default()
                    .width(1)
                    .rounded(3)
                    .color(theme.extended_palette().secondary.weak.color),
            )
        });
        let top = container(button(icon(Lucide::Trash)).on_press(Message::RemoveArchive(index)))
            .padding([0, 15])
            .align_x(Alignment::End)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill);
        match self.status {
            Status::Processing(_) => base.into(),
            _ => hover(base, top).into(),
        }
    }

    pub fn convert(
        &self,
        folder: PathBuf,
        format: Format,
        mut process: impl FnMut(f32),
    ) -> anyhow::Result<()> {
        let file_name = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("获取文件file_name失败"))?;
        let file_base = match self.format {
            Format::Zip => file_name.strip_suffix(".zip").unwrap(),
            Format::TarGz => file_name.strip_suffix(".tar.gz").unwrap(),
            #[cfg(feature = "7z")]
            Format::SevenZ => file_name.strip_suffix(".7z").unwrap(),
        };
        match (self.format, format) {
            (Format::Zip, Format::TarGz) => {
                zip_to_tar_gz(
                    &self.path,
                    folder.join(format!("{}.tar.gz", file_base)),
                    process,
                )?;
            }
            (Format::TarGz, Format::Zip) => {
                tar_gz_to_zip(
                    &self.path,
                    folder.join(format!("{}.zip", file_base)),
                    process,
                )?;
            }
            #[cfg(feature = "7z")]
            (Format::Zip, Format::SevenZ) => {
                zip_to_seven_z(
                    &self.path,
                    folder.join(format!("{}.7z", file_base)),
                    process,
                )?;
            }
            #[cfg(feature = "7z")]
            (Format::SevenZ, Format::Zip) => {
                seven_z_to_zip(
                    &self.path,
                    folder.join(format!("{}.zip", file_base)),
                    process,
                )?;
            }
            #[cfg(feature = "7z")]
            (Format::SevenZ, Format::TarGz) => {
                seven_z_to_tar_gz(
                    &self.path,
                    folder.join(format!("{}.tar.gz", file_base)),
                    process,
                )?;
            }
            #[cfg(feature = "7z")]
            (Format::TarGz, Format::SevenZ) => {
                tar_gz_to_seven_z(
                    &self.path,
                    folder.join(format!("{}.7z", file_base)),
                    process,
                )?;
            }
            _ => {
                let metadata = self.path.metadata()?;
                let total_size = metadata.len();
                let mut read_size = 0u64;
                let source = File::open(&self.path)?;
                let mut target = File::create(folder.join(file_name))?;
                std::io::copy(
                    &mut ProcessRead::new(source, |size| {
                        read_size += size;
                        process((read_size as f64 / total_size as f64) as f32)
                    }),
                    &mut target,
                )?;
            }
        }
        Ok(())
    }
}

macro_rules! io {
    ($source:expr, $target:expr) => {
        (
            BufReader::new(File::open($source)?),
            BufWriter::new(File::create($target)?),
        )
    };
}

///
/// zip压缩包转为tar.gz
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
fn zip_to_tar_gz(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let (source, target) = io!(source, target);
    let mut zip = ZipArchive::new(BufReader::new(source))?;
    let gz = flate2::write::GzEncoder::new(target, Compression::default());
    let mut tar_gz = tar::Builder::new(gz);
    let total_size = zip.decompressed_size().unwrap_or(u128::MAX);
    let mut read_size = 0u64;
    for index in 0..zip.len() {
        let file = zip.by_index(index)?;
        let path = file.name().to_string();
        let mut header = Header::new_gnu();
        header.set_size(file.size());
        header.set_mode(file.unix_mode().unwrap_or(0o644));
        header.set_entry_type(if file.is_dir() {
            EntryType::Directory
        } else {
            EntryType::Regular
        });
        header.set_cksum();
        tar_gz.append_data(
            &mut header,
            path,
            ProcessRead::new(file, |size| {
                read_size += size;
                process((read_size as f64 / total_size as f64) as f32);
            }),
        )?;
    }
    tar_gz.finish()?;
    Ok(())
}

///
/// tar.gz压缩包转为zip压缩包
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
fn tar_gz_to_zip(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let total_size = get_tar_gz_decompress_size(source.as_ref())?;
    let mut read_size = 0u64;
    let (source, target) = io!(source, target);
    let gz = flate2::read::GzDecoder::new(source);
    let mut tar_gz = tar::Archive::new(gz);
    let mut zip = ZipWriter::new(target);
    for entry in tar_gz.entries()? {
        let mut entry = entry?;
        let header = entry.header();
        let path = header.path()?;
        let unix_mode = header.mode()?;
        let options = FileOptions::<()>::default()
            .compression_method(CompressionMethod::Bzip2)
            .unix_permissions(unix_mode);
        if header.entry_type().is_dir() {
            zip.add_directory_from_path(path, options)?
        } else {
            zip.start_file_from_path(path, options)?;
            read_size += std::io::copy(&mut entry, &mut zip)?;
            process((read_size as f64 / total_size as f64) as f32)
        }
    }
    zip.finish().map(|_| ())?;
    Ok(())
}
///
/// tar.gz压缩包转为7z压缩包
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
#[cfg(feature = "7z")]
fn tar_gz_to_seven_z(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let total_size = get_tar_gz_decompress_size(source.as_ref())?;
    let mut read_size = 0u64;
    let (source, target) = io!(source, target);
    let gz = flate2::read::GzDecoder::new(source);
    let mut tar_gz = tar::Archive::new(gz);
    let mut seven_z = sevenz_rust2::ArchiveWriter::new(target)?;
    for entry in tar_gz.entries()? {
        let file = entry?;
        let path = file.path().map(|s| s.display().to_string())?;
        if file.header().entry_type().is_dir() {
            let entry = sevenz_rust2::ArchiveEntry::new_directory(&path);
            seven_z.push_archive_entry::<File>(entry, None)?;
        } else {
            let entry = sevenz_rust2::ArchiveEntry::new_file(&path);
            seven_z.push_archive_entry(
                entry,
                Some(ProcessRead::new(file, |size| {
                    read_size += size;
                    process((size as f64 / total_size as f64) as f32)
                })),
            )?;
        }
    }
    seven_z.finish()?;
    Ok(())
}

///
/// 7z压缩包转为tar.gz压缩包
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
#[cfg(feature = "7z")]
fn seven_z_to_tar_gz(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let (source, target) = io!(source, target);
    let mut seven_z = sevenz_rust2::ArchiveReader::new(source, sevenz_rust2::Password::empty())?;
    let total_size = seven_z
        .archive()
        .files
        .iter()
        .map(|entry| entry.size)
        .sum::<u64>();
    let mut read_size = 0u64;
    let gz = flate2::write::GzEncoder::new(target, Compression::default());
    let mut tar_gz = tar::Builder::new(gz);
    let extract = |entry: &sevenz_rust2::ArchiveEntry, reader: &mut dyn Read| {
        let mut header = Header::new_gnu();
        header.set_size(entry.size());
        header.set_mode(0o644);
        header.set_entry_type(if entry.is_directory {
            EntryType::Directory
        } else {
            EntryType::Regular
        });
        header.set_cksum();
        tar_gz.append_data(
            &mut header,
            &entry.name,
            ProcessRead::new(reader, |size| {
                read_size += size;
                process((read_size as f64 / total_size as f64) as f32);
            }),
        )?;
        Ok(true)
    };
    seven_z.for_each_entries(extract)?;
    tar_gz.finish()?;
    Ok(())
}

///
/// 7z压缩包转为zip压缩包
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
#[cfg(feature = "7z")]
fn seven_z_to_zip(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let (source, target) = io!(source, target);
    let mut seven_z = sevenz_rust2::ArchiveReader::new(source, sevenz_rust2::Password::empty())?;
    let total_size = seven_z
        .archive()
        .files
        .iter()
        .map(|entry| entry.size)
        .sum::<u64>();
    let mut read_size = 0u64;
    let mut zip = ZipWriter::new(target);
    let extract = |entry: &sevenz_rust2::ArchiveEntry, reader: &mut dyn Read| {
        fn map_to_seven_z_error(source: ZipError) -> sevenz_rust2::Error {
            match source {
                ZipError::Io(e) => e.into(),
                ZipError::FileNotFound => sevenz_rust2::Error::FileNotFound,
                _ => sevenz_rust2::Error::Other(source.to_string().into()),
            }
        }

        let options = FileOptions::<()>::default()
            .compression_method(CompressionMethod::Bzip2)
            .unix_permissions(0o644);
        if entry.is_directory {
            zip.add_directory_from_path(entry.name(), options)
                .map_err(map_to_seven_z_error)?;
        } else {
            zip.start_file_from_path(entry.name(), options)
                .map_err(map_to_seven_z_error)?;
            read_size += std::io::copy(reader, &mut zip)?;
            process((read_size as f64 / total_size as f64) as f32)
        }
        Ok(true)
    };
    seven_z.for_each_entries(extract)?;
    zip.finish()?;
    Ok(())
}

///
/// zip压缩包转为7z压缩包
///
/// # Arguments
///
/// * `source`: 源文件
/// * `target`: 目标文件
/// * `process`: 处理进度
///
/// returns: Result<(), Error>
///
#[cfg(feature = "7z")]
fn zip_to_seven_z(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
    mut process: impl FnMut(f32),
) -> anyhow::Result<()> {
    let (source, target) = io!(source, target);
    let mut zip = ZipArchive::new(source)?;
    let total_size = zip.decompressed_size().unwrap_or(u128::MAX);
    let mut read_size = 0u64;
    let mut seven_z = sevenz_rust2::ArchiveWriter::new(target)?;
    for index in 0..zip.len() {
        let file = zip.by_index(index)?;
        if file.is_dir() {
            let entry = sevenz_rust2::ArchiveEntry::new_directory(file.name());
            seven_z.push_archive_entry::<File>(entry, None)?;
        } else {
            let entry = sevenz_rust2::ArchiveEntry::new_file(file.name());
            seven_z.push_archive_entry(
                entry,
                Some(ProcessRead::new(file, |size| {
                    read_size += size;
                    process((read_size as f64 / total_size as f64) as f32)
                })),
            )?;
        }
    }
    seven_z.finish()?;
    Ok(())
}

fn get_tar_gz_decompress_size(source: impl AsRef<Path>) -> anyhow::Result<u64> {
    let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(source)?));
    let mut tar_gz = tar::Archive::new(gz);
    let entries = tar_gz.entries()?;
    Ok(entries
        .into_iter()
        .flatten()
        .filter(|entry| entry.header().entry_type().is_file())
        .flat_map(|entry| entry.header().size())
        .sum())
}

struct ProcessRead<R, F> {
    read: R,
    f: F,
}

impl<R, F> ProcessRead<R, F> {
    pub fn new(read: R, f: F) -> Self {
        Self { read, f }
    }
}

impl<R: Read, F: FnMut(u64)> Read for ProcessRead<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.read.read(buf)?;
        (self.f)(size as u64);
        Ok(size)
    }
}
