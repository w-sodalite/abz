use iced::Color;
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub enum Status {
    #[default]
    Pending,
    Processing(f32),
    Success,
    Failed(Arc<str>),
}

impl Eq for Status {}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> Ordering {
        fn ordinal(status: &Status) -> usize {
            match status {
                Status::Pending => 0,
                Status::Processing(_) => 1,
                Status::Success => 2,
                Status::Failed(_) => 3,
            }
        }

        let o1 = ordinal(self);
        let o2 = ordinal(other);
        if o1 == o2 {
            match (self, other) {
                (Self::Processing(a), Self::Processing(b)) => {
                    a.partial_cmp(b).unwrap_or(Ordering::Equal)
                }
                _ => Ordering::Equal,
            }
        } else {
            o1.cmp(&o2)
        }
    }
}

impl Status {
    pub fn color(&self) -> Color {
        match self {
            Status::Pending => Color::from_rgb(0.42, 0.45, 0.50), // Gray (#6b7280)
            Status::Processing(_) => Color::from_rgb(0.15, 0.39, 0.92), // Blue (#2563eb)
            Status::Success => Color::from_rgb(0.09, 0.64, 0.29), // Green (#16a34a)
            Status::Failed(_) => Color::from_rgb(0.86, 0.15, 0.15), // Red (#dc2626)
        }
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Pending => write!(f, "等待处理"),
            Status::Processing(_) => write!(f, "处理中"),
            Status::Success => write!(f, "处理成功"),
            Status::Failed(e) => f.write_str(&format!("处理失败: {}", e)),
        }
    }
}
