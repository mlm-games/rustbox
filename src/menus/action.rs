use crate::maker::level::LevelTag;

#[derive(Clone, Debug)]
pub enum UiAction {
    StartGame,
    CloseOverlay,
    Resume,
    QuitToTitle,
    QuitApp,
    // Maker pause menu
    MakerRetry,
    MakerCloseSignDialog,
    // Local level browser
    BrowseOpen,
    BrowsePlay(String),
    BrowseEdit(String),
    BrowseDelete(String),
    BrowseConfirmDelete(String),
    BrowseCancelDelete,
    BrowseSelect(String),
    BrowseClearSelection,
    BrowseToggleTag(LevelTag),
    BrowseToggleVerified,
    BrowseSetDifficulty(Option<u8>),
    BrowseCycleSort,
    BrowseSetQuery(String),
    BrowseClearQuery,
    SetKeyboardCaptured(bool),
}
