use crate::archive::Archive;
use crate::format::Format;
use crate::status::Status;
use crate::widget::button::icon_button;
use crate::widget::icon::icon;
use anyhow::anyhow;
use iced::border::Radius;
use iced::task::sipper;
use iced::widget::{
    Column, Space, center, column, container, opaque, pick_list, row, scrollable, stack, text,
    toggler,
};
use iced::{Alignment, Element, Font, Length, Padding, Settings, Task, Theme, Vector, application};
use lucide_rs::Lucide;
use rfd::AsyncFileDialog;
use std::fs::{DirEntry, read_dir};
use std::path::{Path, PathBuf};
use tokio::spawn;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;

const TITLE: &str = "Abz";

const NOTO_SANS_SC: Font = Font::with_name("Noto Sans SC");

#[derive(Debug, Clone)]
pub enum Message {
    UpdateFormat(Format),
    UpdateRecursion(bool),
    PickFiles,
    PickFolder,
    SelectArchives(Vec<Archive>),
    RemoveAll,
    RemoveArchive(usize),
    Convert,
    SelectSaveFolder(Option<PathBuf>),
    Completed,
    UpdateArchiveStatus(usize, Status),
    Error(String),
}

pub struct App {
    format: Format,
    archives: Vec<Archive>,
    loading: bool,
    recursion: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            format: Default::default(),
            archives: vec![],
            loading: false,
            recursion: true,
        }
    }
}

impl App {
    pub fn run() -> iced::Result {
        application(App::new, App::update, App::view)
            .settings(Self::settings())
            .theme(App::theme)
            .title(TITLE)
            .executor::<tokio::runtime::Runtime>()
            .run()
    }

    fn settings() -> Settings {
        let mut settings = Settings::default();
        settings
            .fonts
            .push(include_bytes!("../fonts/NotoSansSC.ttf").into());
        settings.fonts.push(Lucide::font_data().into());
        settings.default_text_size = 12.into();
        settings.default_font = NOTO_SANS_SC;
        settings
    }

    fn theme(&self) -> Theme {
        Theme::Dracula
    }

    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdateFormat(format) => {
                self.format = format;
                Task::none()
            }
            Message::UpdateRecursion(recursion) => {
                self.recursion = recursion;
                Task::none()
            }
            Message::PickFiles => Task::perform(pick_files(), |result| match result {
                Ok(archives) => Message::SelectArchives(archives),
                Err(e) => Message::Error(e.to_string()),
            }),
            Message::PickFolder => {
                Task::perform(pick_folder(self.recursion), |result| match result {
                    Ok(archives) => Message::SelectArchives(archives),
                    Err(e) => Message::Error(e.to_string()),
                })
            }
            Message::SelectArchives(archives) => {
                self.archives.extend(archives);
                self.archives.sort_by_key(|archive| archive.status.clone());
                Task::none()
            }
            Message::RemoveArchive(index) => {
                self.archives.remove(index);
                self.archives.sort_by_key(|archive| archive.size);
                Task::none()
            }
            Message::RemoveAll => {
                self.archives.clear();
                Task::none()
            }
            Message::Convert => Task::perform(pick_save_folder(), Message::SelectSaveFolder),
            Message::SelectSaveFolder(folder) => match folder {
                Some(folder) => {
                    self.loading = true;
                    let format = self.format;
                    let archives = self.archives.clone();
                    let sipper = sipper(move |mut sender| async move {
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                        fn send_status(
                            tx: &UnboundedSender<(usize, Status)>,
                            index: usize,
                            status: Status,
                        ) {
                            tx.send((index, status)).expect("send archive status error")
                        }

                        archives
                            .into_iter()
                            .enumerate()
                            .for_each(|(index, archive)| {
                                let folder = folder.clone();
                                let tx = tx.clone();
                                send_status(&tx, index, Status::Processing(0.));
                                // 在单独的线程中执行转换任务
                                spawn(async move {
                                    let _tx = tx.clone();
                                    match spawn_blocking(move || {
                                        archive.convert(folder, format, |ratio| {
                                            send_status(&_tx, index, Status::Processing(ratio));
                                        })
                                    })
                                    .await
                                    {
                                        Ok(result) => match result {
                                            Ok(_) => send_status(&tx, index, Status::Success),
                                            Err(e) => send_status(
                                                &tx,
                                                index,
                                                Status::Failed(e.to_string().into()),
                                            ),
                                        },
                                        Err(e) => send_status(
                                            &tx,
                                            index,
                                            Status::Failed(e.to_string().into()),
                                        ),
                                    }
                                });
                            });
                        // 这里需要手动释放 tx，否则会阻塞。
                        drop(tx);
                        while let Some((index, status)) = rx.recv().await {
                            sender.send((index, status)).await;
                        }
                    });
                    Task::sip(
                        sipper,
                        |(index, status)| Message::UpdateArchiveStatus(index, status),
                        |_| Message::Completed,
                    )
                }
                None => Task::none(),
            },
            Message::Completed => {
                self.loading = false;
                Task::none()
            }
            Message::UpdateArchiveStatus(index, status) => {
                self.archives[index].status = status;
                Task::none()
            }
            Message::Error(e) => {
                eprintln!("{}", e);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let controls = self.controls();
        let archives = self.archives();
        let main = column![controls, archives]
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(10);
        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Start)
            .align_x(Alignment::Start)
            .padding(10)
            .into()
    }

    fn controls(&self) -> Element<'_, Message> {
        let opens = row![
            icon_button(Lucide::File, "打开文件")
                .width(Length::Fixed(100.))
                .on_press_maybe(if self.loading {
                    None
                } else {
                    Some(Message::PickFiles)
                }),
            icon_button(Lucide::Menu, "打开文件夹")
                .width(Length::Fixed(100.))
                .on_press_maybe(if self.loading {
                    None
                } else {
                    Some(Message::PickFolder)
                }),
            row![
                text("递归目录: "),
                toggler(self.recursion).on_toggle(Message::UpdateRecursion)
            ]
            .width(Length::Fixed(100.))
            .align_y(Alignment::Center)
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let formats = row![
            icon(Lucide::Torus),
            text("目标格式:"),
            pick_list(Format::ALL, Some(self.format), Message::UpdateFormat)
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let actions = row![
            icon_button(Lucide::Play, "转换")
                .width(Length::Fixed(80.))
                .on_press_maybe(if self.loading {
                    None
                } else {
                    Some(Message::Convert)
                }),
            icon_button(Lucide::Trash, "清空")
                .width(Length::Fixed(80.))
                .on_press_maybe(if self.loading {
                    None
                } else {
                    Some(Message::RemoveAll)
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        container(
            row![opens, formats, actions]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .style(container_style)
        .padding(Padding::default().left(10).right(10))
        .height(Length::Fixed(50.))
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }

    fn archives(&self) -> Element<'_, Message> {
        if self.archives.is_empty() {
            center(text("未选中任何文件"))
        } else {
            let items = Column::with_children(
                self.archives
                    .iter()
                    .enumerate()
                    .map(|(index, archive)| archive.view(index)),
            )
            .padding(Padding::default())
            .spacing(10);

            container(
                scrollable(match self.loading {
                    true => mask_layer(items),
                    false => items.into(),
                })
                .spacing(10),
            )
            .padding(10)
        }
        .width(Length::Fill)
        .height(Length::Fill)
        .style(container_style)
        .into()
    }
}

async fn pick_files() -> anyhow::Result<Vec<Archive>> {
    AsyncFileDialog::default()
        .add_filter("Archive Files", &Format::extensions())
        .add_filter("Zip", &[Format::Zip.extension()])
        .add_filter("Tar.gz", &[Format::TarGz.extension()])
        .pick_files()
        .await
        .into_iter()
        .flatten()
        .map(|handle| Archive::parse(handle.path()))
        .try_fold(vec![], |mut archives, archive| match archive {
            Ok(Some(archive)) => {
                archives.push(archive);
                Ok(archives)
            }
            Ok(None) => Ok(archives),
            Err(e) => Err(e),
        })
        .map_err(|e| anyhow!(e))
}

async fn pick_folder(recursion: bool) -> anyhow::Result<Vec<Archive>> {
    match AsyncFileDialog::default().pick_folder().await {
        None => Ok(vec![]),
        Some(folder) => {
            let entries = list_archive_entry(folder.path(), recursion)?;
            entries
                .into_iter()
                .try_fold(vec![], |mut items, entry| {
                    match Archive::parse(entry.path()) {
                        Ok(file) => {
                            if let Some(file) = file {
                                items.push(file);
                            }
                            Ok(items)
                        }
                        Err(e) => Err(e),
                    }
                })
                .map_err(|e| anyhow!(e))
        }
    }
}

fn list_archive_entry(folder: &Path, recursion: bool) -> anyhow::Result<Vec<DirEntry>> {
    let entries = read_dir(folder).map_err(|e| anyhow!(e))?;
    match recursion {
        true => entries
            .into_iter()
            .try_fold(vec![], |mut dirs, entry| match entry {
                Ok(entry) => match entry.file_type() {
                    Ok(file_type) => {
                        if file_type.is_dir() {
                            match list_archive_entry(&entry.path(), recursion) {
                                Ok(sub_dirs) => {
                                    dirs.extend(sub_dirs);
                                    Ok(dirs)
                                }
                                Err(e) => Err(anyhow!(e)),
                            }
                        } else {
                            dirs.push(entry);
                            Ok(dirs)
                        }
                    }
                    Err(e) => Err(anyhow!(e)),
                },
                Err(e) => Err(anyhow!(e)),
            }),
        false => entries
            .into_iter()
            .try_fold(vec![], |mut entries, entry| match entry {
                Ok(entry) => {
                    entries.push(entry);
                    Ok(entries)
                }
                Err(e) => Err(anyhow!(e)),
            }),
    }
}

async fn pick_save_folder() -> Option<PathBuf> {
    AsyncFileDialog::default()
        .set_title("选择保存的文件夹")
        .set_can_create_directories(true)
        .pick_folder()
        .await
        .map(|h| h.path().to_path_buf())
}

fn container_style(theme: &Theme) -> container::Style {
    let mut base = container::rounded_box(theme);
    base.shadow.offset = Vector::new(0.0, -5.0);
    base.border.radius = Radius::default().top(5).bottom(5);
    base
}

fn mask_layer<'a, M: 'a>(element: impl Into<Element<'a, M>>) -> Element<'a, M> {
    stack(vec![
        element.into(),
        opaque(Space::new(Length::Fill, Length::Fill)),
    ])
    .into()
}
