//! Embedded asset source: the handful of Lucide icons (ISC license) that
//! gpui-component widgets request at runtime (e.g. the Select chevron).
//! Everything is compiled into the binary — no bundle-relative lookups, so
//! the same binary works from a terminal and from a Spotlight-launched .app.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/chevron-down.svg",
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    ("icons/check.svg", include_bytes!("../assets/icons/check.svg")),
    ("icons/inbox.svg", include_bytes!("../assets/icons/inbox.svg")),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
