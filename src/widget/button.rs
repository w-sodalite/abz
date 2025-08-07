use crate::widget::icon::icon;
use iced::Alignment;
use iced::widget::{Button, button, row, text};
use lucide_rs::Lucide;

pub fn icon_button<'a, M>(code: Lucide, label: &'a str) -> Button<'a, M>
where
    M: 'a,
{
    button(
        row![
            icon(code)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
            text(label)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        ]
        .spacing(5),
    )
}
