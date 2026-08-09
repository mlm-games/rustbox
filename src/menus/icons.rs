use repose_core::{ImageHandle, RenderContext};

const BLOCK_ICONS: [&[u8]; 17] = [
    include_bytes!("../../assets/images/blocks/grass.png"),
    include_bytes!("../../assets/images/blocks/stone.png"),
    include_bytes!("../../assets/images/blocks/hazard.png"),
    include_bytes!("../../assets/images/blocks/goal.png"),
    include_bytes!("../../assets/images/blocks/spawn.png"),
    include_bytes!("../../assets/images/blocks/water.png"),
    include_bytes!("../../assets/images/blocks/ice.png"),
    include_bytes!("../../assets/images/blocks/spikes.png"),
    include_bytes!("../../assets/images/blocks/conveyor.png"),
    include_bytes!("../../assets/images/blocks/bounce.png"),
    include_bytes!("../../assets/images/blocks/climb.png"),
    include_bytes!("../../assets/images/blocks/thin_conveyor.png"),
    include_bytes!("../../assets/images/blocks/onoff_conveyor_a.png"),
    include_bytes!("../../assets/images/blocks/onoff_conveyor_b.png"),
    include_bytes!("../../assets/images/blocks/hang_rail.png"),
    include_bytes!("../../assets/images/blocks/one_way.png"),
    include_bytes!("../../assets/images/blocks/timed_pulse.png"),
];

const ENTITY_ICONS: [&[u8]; 22] = [
    include_bytes!("../../assets/images/entities/glimmer.png"),
    include_bytes!("../../assets/images/entities/launch_pad.png"),
    include_bytes!("../../assets/images/entities/seal.png"),
    include_bytes!("../../assets/images/entities/drift_plate.png"),
    include_bytes!("../../assets/images/entities/prowler.png"),
    include_bytes!("../../assets/images/entities/trigger_orb.png"),
    include_bytes!("../../assets/images/entities/relay_gate.png"),
    include_bytes!("../../assets/images/entities/checkpoint.png"),
    include_bytes!("../../assets/images/entities/teleporter.png"),
    include_bytes!("../../assets/images/entities/fan.png"),
    include_bytes!("../../assets/images/entities/bumper.png"),
    include_bytes!("../../assets/images/entities/crate.png"),
    include_bytes!("../../assets/images/entities/key.png"),
    include_bytes!("../../assets/images/entities/lock_gate.png"),
    include_bytes!("../../assets/images/entities/heal_orb.png"),
    include_bytes!("../../assets/images/entities/speed_ring.png"),
    include_bytes!("../../assets/images/entities/crumble_plate.png"),
    include_bytes!("../../assets/images/entities/cannon.png"),
    include_bytes!("../../assets/images/entities/on_off_switch.png"),
    include_bytes!("../../assets/images/entities/toss_crate.png"),
    include_bytes!("../../assets/images/entities/sign.png"),
    include_bytes!("../../assets/images/entities/wedge.png"),
];

fn register(rc: &RenderContext, list: &[&[u8]]) -> Vec<ImageHandle> {
    list.iter()
        .map(|bytes| {
            let handle = rc.alloc_image_handle();
            // Icons are long-lived and not drawn while off the maker palette;
            // the renderer retains RGBA sources so eviction only frees GPU
            // memory and re-uploads them lazily when next drawn.
            rc.set_image_encoded(handle, bytes.to_vec(), true);
            handle
        })
        .collect()
}

pub fn register_block_icons(rc: &RenderContext) -> Vec<ImageHandle> {
    register(rc, &BLOCK_ICONS)
}

pub fn register_entity_icons(rc: &RenderContext) -> Vec<ImageHandle> {
    register(rc, &ENTITY_ICONS)
}
