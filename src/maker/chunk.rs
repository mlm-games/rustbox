use bevy::prelude::*;

pub const CHUNK_SIZE: i32 = 16;

pub fn chunk_of(cell: IVec3) -> IVec3 {
    IVec3::new(
        cell.x.div_euclid(CHUNK_SIZE),
        cell.y.div_euclid(CHUNK_SIZE),
        cell.z.div_euclid(CHUNK_SIZE),
    )
}

pub fn affected_chunks(cell: IVec3) -> Vec<IVec3> {
    let base = chunk_of(cell);
    let local = IVec3::new(
        cell.x.rem_euclid(CHUNK_SIZE),
        cell.y.rem_euclid(CHUNK_SIZE),
        cell.z.rem_euclid(CHUNK_SIZE),
    );
    let mut out = vec![base];
    for axis in 0..3 {
        if local[axis] == 0 {
            let mut n = base;
            n[axis] -= 1;
            out.push(n);
        }
        if local[axis] == CHUNK_SIZE - 1 {
            let mut n = base;
            n[axis] += 1;
            out.push(n);
        }
    }
    out
}
