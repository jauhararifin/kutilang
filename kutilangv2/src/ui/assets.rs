//! Icons: the default set of the component library, plus the few Lucide icons
//! this application uses that are not part of it (see `assets/icons`, ISC
//! licensed). Embedding the whole Lucide catalog would add 7 MB.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

macro_rules! extra_icons {
    ($($name:literal),* $(,)?) => {
        [$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../../assets/icons/", $name, ".svg")) as &[u8])),*]
    };
}

const EXTRA_ICONS: [(&str, &[u8]); 10] = extra_icons!(
    "arrow-down-a-z",
    "bold",
    "clock",
    "pencil",
    "printer",
    "rotate-ccw",
    "stethoscope",
    "trash",
    "user-plus",
    "users",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, data)) = EXTRA_ICONS.iter().find(|(icon, _)| *icon == path) {
            return Ok(Some(Cow::Borrowed(data)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(
            EXTRA_ICONS
                .iter()
                .filter(|(icon, _)| icon.starts_with(path))
                .map(|(icon, _)| SharedString::from(*icon)),
        );
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::assets::IconName;

    #[test]
    fn serves_the_icons_the_interface_uses() {
        for icon in [
            IconName::ArrowDownAZ,
            IconName::Bold,
            IconName::Calendar,
            IconName::Clock,
            IconName::Eye,
            IconName::Inbox,
            IconName::Pencil,
            IconName::Plus,
            IconName::Printer,
            IconName::RotateCcw,
            IconName::Search,
            IconName::Settings,
            IconName::Stethoscope,
            IconName::Trash,
            IconName::User,
            IconName::UserPlus,
            IconName::Users,
        ] {
            let path = icon.path();
            assert!(
                matches!(Assets.load(&path), Ok(Some(data)) if !data.is_empty()),
                "missing {path}"
            );
        }
    }
}
