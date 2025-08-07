use iced::widget::{Text, text};
use lucide_rs::Lucide;

pub fn icon<'a>(code: Lucide) -> Text<'a> {
    text(code).font(Lucide::FONT)
}
