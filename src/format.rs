use std::fmt::{Display, Formatter};

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Format {
    #[default]
    Zip,
    TarGz,
    #[cfg(feature = "7z")]
    SevenZ,
}

impl Format {
    #[cfg(feature = "7z")]
    pub const ALL: [Format; 3] = [Format::Zip, Format::SevenZ, Format::TarGz];

    #[cfg(not(feature = "7z"))]
    pub const ALL: [Format; 2] = [Format::Zip, Format::TarGz];

    #[cfg(feature = "7z")]
    pub fn extensions() -> [&'static str; 3] {
        ["zip", "7z", "tar.gz"]
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::TarGz => "tar.gz",
            #[cfg(feature = "7z")]
            Format::SevenZ => "7z",
        }
    }

    #[cfg(not(feature = "7z"))]
    pub fn extensions() -> [&'static str; 2] {
        ["zip", "tar.gz"]
    }

    pub fn parse(file_name: &str) -> Option<Format> {
        if file_name.ends_with(".zip") {
            return Some(Format::Zip);
        }
        if file_name.ends_with(".tar.gz") {
            return Some(Format::TarGz);
        }
        #[cfg(feature = "7z")]
        if file_name.ends_with(".7z") {
            return Some(Format::SevenZ);
        }
        None
    }
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Zip => write!(f, "zip"),
            Format::TarGz => write!(f, "tar.gz"),
            #[cfg(feature = "7z")]
            Format::SevenZ => write!(f, "7z"),
        }
    }
}
