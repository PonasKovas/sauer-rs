use crate::Map;
use sauer_proto::Encode;
use sauer_proto::channel1::ClipboardServer;
use sauer_proto::data::ClipboardPayload;
use sauer_proto::data::MAX_CLIENT_UNPACK_LEN;
use sauer_proto::route::{Chan1, S2C};

/// Splits the region into clipboards whose packets fit in `max_bytes`
/// (compressed) and that clients accept (unpacked size). Pass `usize::MAX`
/// for as few clipboards as clients allow.
pub fn chunk_region(
    map: &Map,
    min: [i32; 3],
    max: [i32; 3],
    max_bytes: usize,
) -> Vec<ClipboardPayload> {
    let mut out = Vec::new();
    do_chunk(map, min, max, max_bytes, &mut out);
    out
}

fn do_chunk(
    map: &Map,
    min: [i32; 3],
    max: [i32; 3],
    max_bytes: usize,
    out: &mut Vec<ClipboardPayload>,
) {
    let mut grid = 1i32;
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_span = spans.iter().copied().max().unwrap_or(1).max(1);
    while (max_span + grid - 1) / grid > 8 {
        grid <<= 1;
    }
    let shaped = crate::octree::max_shaped_leaf(&map.root, [0; 3], map.size, min, max);
    grid = grid.max(shaped);

    let padded_min: [i32; 3] = std::array::from_fn(|i| min[i].div_euclid(grid) * grid);
    let padded_max: [i32; 3] =
        std::array::from_fn(|i| (((max[i] + grid - 1).div_euclid(grid)) * grid).min(map.size));

    let sx = (padded_max[0] - padded_min[0]) / grid;
    let sy = (padded_max[1] - padded_min[1]) / grid;
    let sz = (padded_max[2] - padded_min[2]) / grid;

    // Check if it's 1x1x1 and cannot be split further
    // Going smaller than `shaped` would split a shaped leaf.
    let cannot_split = sx <= 1 && sy <= 1 && sz <= 1 && grid <= shaped.max(1);

    let payload =
        crate::clipboard::generate_clipboard_for_region(map, padded_min, [sx, sy, sz], grid);

    let fits = payload.uncompressed().len() <= MAX_CLIENT_UNPACK_LEN
        && (max_bytes == usize::MAX || {
            let packet = ClipboardServer {
                client_num: 0,
                payload: Some(payload.clone()),
            };
            let mut buf = Vec::new();
            <ClipboardServer as Encode<S2C<Chan1>>>::encode(&packet, &mut buf);
            buf.len() <= max_bytes
        });

    if fits || cannot_split {
        out.push(payload);
    } else {
        let mid = [
            min[0] + (max[0] - min[0]) / 2,
            min[1] + (max[1] - min[1]) / 2,
            min[2] + (max[2] - min[2]) / 2,
        ];

        let mut sub_boxes = Vec::new();
        for i in 0..8 {
            let cx = i & 1;
            let cy = (i >> 1) & 1;
            let cz = (i >> 2) & 1;

            let c_min = [
                if cx == 0 { min[0] } else { mid[0] },
                if cy == 0 { min[1] } else { mid[1] },
                if cz == 0 { min[2] } else { mid[2] },
            ];

            let c_max = [
                if cx == 0 { mid[0] } else { max[0] },
                if cy == 0 { mid[1] } else { max[1] },
                if cz == 0 { mid[2] } else { max[2] },
            ];

            if c_min[0] < c_max[0] && c_min[1] < c_max[1] && c_min[2] < c_max[2] {
                sub_boxes.push((c_min, c_max));
            }
        }

        for (c_min, c_max) in sub_boxes {
            do_chunk(map, c_min, c_max, max_bytes, out);
        }
    }
}
