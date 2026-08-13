use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;

use crate::app::AppState;
use crate::asset_tracking::AssetsLoading;
use game_utils_bevy::transitions::Transition;

pub struct ScreensPlugin;
impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Splash), |mut c: Commands| {
            c.insert_resource(SplashTimer(Timer::from_seconds(1.5, TimerMode::Once)));
        })
        .add_systems(
            OnEnter(AppState::Loading),
            (|mut c: Commands, asset_server: Res<AssetServer>| {
                c.insert_resource(LoadingTimer(Timer::from_seconds(0.5, TimerMode::Once)));
                let scenes = [
                    "models/cubeworld/Character_Male_2.gltf#Scene0",
                    "models/rustbox/entities/Glimmer.glb#Scene0",
                    "models/rustbox/entities/LaunchPad.glb#Scene0",
                    "models/rustbox/entities/Seal.glb#Scene0",
                    "models/rustbox/entities/DriftPlate.glb#Scene0",
                    "models/rustbox/entities/Prowler.glb#Scene0",
                    "models/rustbox/entities/TriggerOrb.glb#Scene0",
                    "models/rustbox/entities/RelayGate.glb#Scene0",
                    "models/rustbox/entities/Checkpoint.glb#Scene0",
                    "models/rustbox/entities/Teleporter.glb#Scene0",
                    "models/rustbox/entities/Fan.glb#Scene0",
                    "models/rustbox/entities/Bumper.glb#Scene0",
                    "models/rustbox/entities/Crate.glb#Scene0",
                    "models/rustbox/entities/Key.glb#Scene0",
                    "models/rustbox/entities/LockGate.glb#Scene0",
                    "models/rustbox/entities/HealOrb.glb#Scene0",
                    "models/rustbox/entities/SpeedRing.glb#Scene0",
                    "models/rustbox/entities/CrumblePlate.glb#Scene0",
                    "models/rustbox/entities/Cannon.glb#Scene0",
                    "models/rustbox/entities/OnOffSwitch.glb#Scene0",
                    "models/rustbox/entities/TossCrate.glb#Scene0",
                    "models/rustbox/entities/Sign.glb#Scene0",
                    "models/rustbox/entities/Wedge.glb#Scene0",
                ];
                let mut handles = Vec::new();
                for path in scenes {
                    handles.push(asset_server.load::<WorldAsset>(path).untyped());
                }
                c.insert_resource(AssetsLoading(handles));
            },)
                .chain(),
        )
        .add_systems(Update, (tick_splash, tick_loading));
    }
}

#[derive(Resource)]
struct SplashTimer(Timer);
#[derive(Resource)]
struct LoadingTimer(Timer);

fn tick_splash(
    time: Res<Time<Real>>,
    mut tr: ResMut<Transition<AppState>>,
    timer: Option<ResMut<SplashTimer>>,
) {
    let Some(mut timer) = timer else { return };
    if timer.0.tick(time.delta()).just_finished() {
        tr.begin_to_state(AppState::Title);
    }
}

fn tick_loading(
    time: Res<Time<Real>>,
    mut tr: ResMut<Transition<AppState>>,
    asset_server: Res<AssetServer>,
    timer: Option<ResMut<LoadingTimer>>,
    assets: Option<Res<AssetsLoading>>,
) {
    let Some(mut timer) = timer else { return };
    let loaded = assets
        .map(|a| {
            a.0.iter()
                .all(|h| asset_server.is_loaded_with_dependencies(h))
        })
        .unwrap_or(true);
    if loaded && timer.0.tick(time.delta()).just_finished() {
        tr.begin_to_state(AppState::InGame);
    }
}
