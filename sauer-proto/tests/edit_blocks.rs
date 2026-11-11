use cube_string::CubeString;
use sauer_core::{Cube, Material, Node};
use sauer_proto::channel1::{ClipboardClient, Replace, UndoClient};
use sauer_proto::data::{
    ClipboardPayload, Orient, SelInfo, TexRef, UndoPayload, VSlotData, VSlotParam, VSlotProps, Vec3,
};
use sauer_proto::route::{C2S, Chan1};
use sauer_proto::{Chan1Client, Reader, Session, Writer};

fn sel() -> SelInfo {
    SelInfo {
        o: Vec3 { x: 8, y: 16, z: 24 },
        s: Vec3 { x: 2, y: 1, z: 1 },
        grid: 8,
        orient: Orient::Top,
        cx: 0,
        cxs: 0,
        cy: 0,
        cys: 0,
        corner: 0,
    }
}

fn props() -> VSlotProps {
    VSlotProps {
        params: vec![VSlotParam {
            name: CubeString::from_str_lossy("glowcolor"),
            values: [1.0, 0.5, 0.25, 0.0],
        }],
        rotation: Some(3),
        color: Some((0.5, 1.0, 1.0)),
        ..Default::default()
    }
}

fn nodes() -> Vec<Node> {
    let leaf = |tex| {
        Node::Leaf(Cube {
            edges: [0x80; 12],
            textures: [tex; 6],
            material: Material::from_bits(0),
        })
    };
    vec![
        leaf(5),
        Node::Branch(Box::new(std::array::from_fn(|i| leaf(i as u16)))),
    ]
}

#[test]
fn edit_blocks_roundtrip() {
    let replace = Replace {
        sel: sel(),
        old_tex: TexRef::Index(7),
        new_tex: TexRef::Variant {
            slot: 12,
            vslot: props(),
        },
        in_sel: true,
    };
    let clipboard = ClipboardClient {
        payload: Some(ClipboardPayload {
            sel: sel(),
            nodes: nodes(),
            vslots: vec![VSlotData {
                index: 5,
                slot: 2,
                vslot: props(),
            }],
        }),
    };
    let cleared = ClipboardClient { payload: None };
    let undo = UndoClient {
        payload: UndoPayload::Cubes {
            sel: sel(),
            nodes: nodes(),
            gridmap: vec![8, 8],
            vslots: vec![],
        },
    };

    let mut writer = Writer::<C2S<Chan1>>::new();
    writer.push(&replace);
    writer.push(&clipboard);
    writer.push(&cleared);
    writer.push(&undo);
    let (bytes, _) = writer.consume();

    let mut session = Session::new();
    let mut reader = Reader::<C2S<Chan1>>::new(&bytes, &mut session);
    let mut got: Vec<Chan1Client> = Vec::new();
    while let Some(msg) = reader.next() {
        got.push(msg.unwrap());
    }
    assert_eq!(
        got,
        vec![
            Chan1Client::Replace(replace),
            Chan1Client::ClipboardClient(clipboard),
            Chan1Client::ClipboardClient(cleared),
            Chan1Client::UndoClient(undo),
        ]
    );
}
