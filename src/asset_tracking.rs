use bevy::asset::LoadState;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct AssetsLoading(pub Vec<UntypedHandle>);

/// A dependency counts as settled once it is loaded or has failed. Failed
/// loads use procedural fallbacks elsewhere, so they must not block progress.
pub fn asset_failed(asset_server: &AssetServer, handle: &UntypedHandle) -> bool {
    matches!(asset_server.load_state(handle.id()), LoadState::Failed(_))
}
