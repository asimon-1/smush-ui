use super::{binary::DictionaryNodeDisk, bntx_error};
use binrw::BinResult;

const ROOT_REFERENCE: u32 = u32::MAX;

struct Node {
    disk: DictionaryNodeDisk,
    key_index: Option<usize>,
}

pub(crate) fn build(
    keys: &[(&str, u64)],
    empty_string_offset: u64,
) -> BinResult<Vec<DictionaryNodeDisk>> {
    if keys.len() > usize::from(u16::MAX) {
        return Err(bntx_error(0, "BNTX dictionary exceeds 65535 entries"));
    }

    let mut nodes = vec![Node {
        disk: DictionaryNodeDisk {
            reference: ROOT_REFERENCE,
            left: 0,
            right: 0,
            key_offset: empty_string_offset,
        },
        key_index: None,
    }];

    for (key_index, &(key, key_offset)) in keys.iter().enumerate() {
        let key_bytes = key.as_bytes();
        let terminal = find_terminal(&nodes, key_bytes)?;
        let terminal_key = nodes[usize::from(terminal)]
            .key_index
            .map_or(&[][..], |index| keys[index].0.as_bytes());
        let reference = first_differing_bit(terminal_key, key_bytes)
            .ok_or_else(|| bntx_error(0, format!("duplicate BNTX dictionary key {key:?}")))?;

        let new_index = u16::try_from(nodes.len())
            .map_err(|_| bntx_error(0, "BNTX dictionary node index exceeds 65535"))?;
        let mut parent = 0u16;
        let mut child = nodes[0].disk.left;
        let mut parent_uses_right = false;
        while signed_reference(nodes[usize::from(child)].disk.reference)
            < signed_reference(reference)
            && signed_reference(nodes[usize::from(child)].disk.reference)
                > signed_reference(nodes[usize::from(parent)].disk.reference)
        {
            parent = child;
            parent_uses_right = key_bit(key_bytes, nodes[usize::from(child)].disk.reference) != 0;
            child = branch(&nodes[usize::from(child)].disk, key_bytes);
        }

        let mut disk = DictionaryNodeDisk {
            reference,
            left: child,
            right: child,
            key_offset,
        };
        if key_bit(key_bytes, reference) == 0 {
            disk.left = new_index;
        } else {
            disk.right = new_index;
        }
        nodes.push(Node {
            disk,
            key_index: Some(key_index),
        });
        if parent_uses_right {
            nodes[usize::from(parent)].disk.right = new_index;
        } else {
            nodes[usize::from(parent)].disk.left = new_index;
        }
    }

    Ok(nodes.into_iter().map(|node| node.disk).collect())
}

fn find_terminal(nodes: &[Node], key: &[u8]) -> BinResult<u16> {
    let mut previous = signed_reference(nodes[0].disk.reference);
    let mut index = nodes[0].disk.left;
    for _ in 0..=nodes.len() {
        let node = &nodes[usize::from(index)];
        let reference = signed_reference(node.disk.reference);
        if reference <= previous {
            return Ok(index);
        }
        previous = reference;
        index = branch(&node.disk, key);
    }
    Err(bntx_error(
        0,
        "BNTX dictionary construction produced a cycle",
    ))
}

fn first_differing_bit(left: &[u8], right: &[u8]) -> Option<u32> {
    let bit_count = u32::try_from(left.len().max(right.len()).checked_mul(8)?).ok()?;
    (0..bit_count).find(|&bit| key_bit(left, bit) != key_bit(right, bit))
}

fn branch(node: &DictionaryNodeDisk, key: &[u8]) -> u16 {
    if key_bit(key, node.reference) == 0 {
        node.left
    } else {
        node.right
    }
}

fn key_bit(key: &[u8], bit: u32) -> u8 {
    let byte_index = (bit / 8) as usize;
    if byte_index >= key.len() {
        0
    } else {
        (key[key.len() - 1 - byte_index] >> (bit % 8)) & 1
    }
}

fn signed_reference(reference: u32) -> i32 {
    reference as i32
}
