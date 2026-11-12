//! The entries of the navigation column, as data: a provider's deep link
//! names one, and the shell draws it.

/// Every navigable entry, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Music,
    Explore,
    Playlists,
    Albums,
    Tracks,
    Profiles,
    Settings,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Music,
        Section::Explore,
        Section::Playlists,
        Section::Albums,
        Section::Tracks,
        Section::Profiles,
        Section::Settings,
    ];

    /// The key that reaches this entry: 1 for Music, 7 for Settings.
    pub fn number(&self) -> usize {
        Self::ALL.iter().position(|s| s == self).unwrap_or(0) + 1
    }
}
