//! The game's GPS navigator as its own code lays it out: where its fields are, read from the instructions that
//! use them, so a game version that moves them (ATS 1.62 moved everything from `+0x118` up by 0x10 against
//! 1.61) is read as it is. Two of the navigator's functions keep their shape across versions, only their
//! offsets change; each is found by its shape with the offsets left open, and the offsets are read from it:
//!
//! - its "set targets" (`navigator, mode, four lists`): `mov [rcx+mode], edx`, `add rcx, first list`, then
//!   `lea rcx, [rdi+list]` for each of the other three, in order: the lists are the one it plans into, the
//!   route's targets, the places to avoid and the waypoints (1.61: `+0x1C8`; `+0x1D0`, `+0x1F8`, `+0x220`,
//!   `+0x248`);
//! - its "next node" getter: `cmp eax, [rbx+id]`, then `mov rax, [rbx+next]`: the node of the route ahead
//!   of the truck and the route id it's for (1.61: `+0x198`, `+0x190`);
//! - its walk back along a route's nodes, stepping by a node's size (1.61: 0x30; ATS 1.62: 0x38, measured on a
//!   route in the running game too: each node's kind at `+0`, its map node at `+0x08`, its distance left at
//!   `+0x24`, as before).
//!
//! Used by the plugin in the game (crates/plugin navigation.rs) and by the app reading the game's file
//! (crates/host route.rs): one reading of the game's code for both.

/// The start of "set targets": `mov [rsp+8], rbx; mov [rsp+10h], rsi; push rdi; sub rsp, 20h; mov rdi, rcx;
/// mov [rcx+mode], edx; add rcx, first` (`None`: an offset, read from it).
#[rustfmt::skip]
pub const SET_TARGETS: [Option<u8>; 31] = [
    Some(0x48), Some(0x89), Some(0x5c), Some(0x24), Some(0x08), // mov [rsp+8], rbx
    Some(0x48), Some(0x89), Some(0x74), Some(0x24), Some(0x10), // mov [rsp+10h], rsi
    Some(0x57), // push rdi
    Some(0x48), Some(0x83), Some(0xec), Some(0x20), // sub rsp, 20h
    Some(0x48), Some(0x8b), Some(0xf9), // mov rdi, rcx
    Some(0x89), Some(0x91), None, None, Some(0x00), Some(0x00), // mov [rcx+mode], edx
    Some(0x48), Some(0x81), Some(0xc1), None, None, Some(0x00), Some(0x00), // add rcx, first
];

/// The "next node" getter: `push rbx; sub rsp, 20h; mov rax, [rcx+8]; mov rbx, rcx; add rcx, 8; call [rax+30h]`
/// (the route's id), `cmp eax, [rbx+id]; jne …; mov rax, [rbx+next]`.
#[rustfmt::skip]
pub const NEXT_NODE: [Option<u8>; 35] = [
    Some(0x40), Some(0x53), // push rbx
    Some(0x48), Some(0x83), Some(0xec), Some(0x20), // sub rsp, 20h
    Some(0x48), Some(0x8b), Some(0x41), Some(0x08), // mov rax, [rcx+8]
    Some(0x48), Some(0x8b), Some(0xd9), // mov rbx, rcx
    Some(0x48), Some(0x83), Some(0xc1), Some(0x08), // add rcx, 8
    Some(0xff), Some(0x50), Some(0x30), // call [rax+30h]
    Some(0x3b), Some(0x83), None, None, Some(0x00), Some(0x00), // cmp eax, [rbx+id]
    Some(0x75), None, // jne short
    Some(0x48), Some(0x8b), Some(0x83), None, None, Some(0x00), Some(0x00), // mov rax, [rbx+next]
];

/// A walk back along a route's nodes from its end (the navigator's, checking each node's kind, 0 or 1, and
/// what it points to): `mov r13, [r11+8]` (the nodes), `lea rbx, [r13-size]`, … `sub reg, size`, the size of a
/// route node twice (1.61: 0x30; ATS 1.62: 0x38). Its registers and the size are left open.
#[rustfmt::skip]
pub const ROUTE_WALK: [Option<u8>; 59] = [
    Some(0x4d), Some(0x8b), Some(0x6b), Some(0x08), // mov r13, [r11+8]
    Some(0x49), Some(0x8d), Some(0x5d), None, // lea rbx, [r13-size]
    Some(0x4c), Some(0x3b), Some(0xe3), // cmp r12, rbx
    Some(0x74), None, // je short
    Some(0x49), Some(0x83), Some(0xfa), Some(0x02), // cmp r10, 2
    Some(0x73), None, // jae short
    Some(0x44), Some(0x8b), None, // mov r9d, [node]
    Some(0x45), Some(0x85), Some(0xc9), // test r9d, r9d
    Some(0x74), None, // je short
    Some(0x41), Some(0x83), Some(0xf9), Some(0x01), // cmp r9d, 1
    Some(0x75), None, // jne short
    Some(0x48), Some(0x39), None, Some(0x08), // cmp [node+8], rdi
    Some(0xeb), None, // jmp short
    Some(0x48), Some(0x8b), None, Some(0x08), // mov rax, [node+8]
    Some(0x48), Some(0x39), Some(0x78), Some(0x28), // cmp [rax+28h], rdi
    Some(0x74), None, // je short
    Some(0x48), Some(0x39), Some(0x78), Some(0x20), // cmp [rax+20h], rdi
    Some(0x74), None, // je short
    Some(0x48), Some(0x83), None, None, // sub node, size
];

/// A route node's size, from the walk along a route's nodes ([`ROUTE_WALK`], `code` starting at it); `None`
/// unless it's shaped so and both places say the same size (a whole number of 8 bytes, at least 0x20).
pub fn read_route_walk(code: &[u8]) -> Option<usize> {
    if !fits(code, &ROUTE_WALK) {
        return None;
    }
    let before = usize::from(code[7].wrapping_neg());
    let step = usize::from(code[58]);
    let adds_up = (code[57] & 0xf8) == 0xe8 && before == step && step >= 0x20 && step.is_multiple_of(8);
    adds_up.then_some(step)
}

/// The navigator's fields "set targets" sets, as its code reads them (offsets into the navigator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Targets {
    /// The routing mode (a u32).
    pub mode: usize,
    /// The list the game plans into (given empty).
    pub planned: usize,
    /// The route's targets in route order (then, on a job, its destination).
    pub route: usize,
    /// The places to avoid.
    pub avoid: usize,
    /// The waypoints in the order they were added.
    pub waypoints: usize,
}

/// The navigator's node of the route ahead, as its getter reads it: the route id it's for (a u32) and the
/// node's pointer (offsets into the navigator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NextNode {
    pub id: usize,
    pub node: usize,
}

/// Where `pattern` starts in `code`, if exactly once (`None` in it: any byte).
pub fn find_unique(code: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
    let fits = |window: &[u8]| window.iter().zip(pattern).all(|(byte, wanted)| wanted.is_none_or(|w| *byte == w));
    let mut found = code.windows(pattern.len()).enumerate().filter(|(_, window)| fits(window));
    let (at, _) = found.next()?;
    found.next().is_none().then_some(at)
}

fn u32_at(code: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(code.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// The fields "set targets" sets, from its code (`code` starting at the function's start): its mode and first
/// list from its first instructions, then its other lists in the order it loads them (`lea rcx, [rdi+list]`,
/// `48 8D 8F disp32`); `None` unless it's shaped so and adds up (the mode just before the first list, the
/// four lists one size apart).
pub fn read_set_targets(code: &[u8]) -> Option<Targets> {
    if !fits(code, &SET_TARGETS) {
        return None;
    }
    let (mode, planned) = (u32_at(code, 20)?, u32_at(code, 27)?);
    let body = code.get(SET_TARGETS.len()..(SET_TARGETS.len() + 0x80).min(code.len()))?;
    let mut lists = (0..body.len().saturating_sub(6))
        .filter(|&at| body[at..at + 3] == [0x48, 0x8d, 0x8f])
        .filter_map(|at| u32_at(body, at + 3));
    let (route, avoid, waypoints) = (lists.next()?, lists.next()?, lists.next()?);
    let size = route.checked_sub(planned)?;
    let adds_up = mode + 8 == planned
        && (8..=0x100).contains(&size)
        && avoid.checked_sub(route) == Some(size)
        && waypoints.checked_sub(avoid) == Some(size);
    adds_up.then_some(Targets { mode, planned, route, avoid, waypoints })
}

/// The next node's fields, from its getter's code (`code` starting at the function's start); `None` unless
/// it's shaped so and adds up (the pointer just after the id it's for).
pub fn read_next_node(code: &[u8]) -> Option<NextNode> {
    if !fits(code, &NEXT_NODE) {
        return None;
    }
    let (id, node) = (u32_at(code, 22)?, u32_at(code, 31)?);
    (id + 8 == node).then_some(NextNode { id, node })
}

fn fits(code: &[u8], pattern: &[Option<u8>]) -> bool {
    code.len() >= pattern.len() && code.iter().zip(pattern).all(|(byte, wanted)| wanted.is_none_or(|w| *byte == w))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Set targets" as ETS2 1.61.1.9 has it (its first 0x73 bytes, from the game's executable).
    const SET_TARGETS_1_61: &str = "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f9 89 91 c8 01 00 00 48 81 c1 \
        d0 01 00 00 33 f6 49 8b d9 4c 3b c1 74 0c 49 8b d0 48 89 71 10 e8 db 3a 00 00 48 8d 8f f8 01 00 00 48 3b d9 \
        74 0c 48 8b d3 48 89 71 10 e8 c3 3a 00 00 48 8b 54 24 50 48 8d 8f 20 02 00 00 48 3b d1 74 09 48 89 71 10 e8 \
        a9 3a 00 00 48 8b 54 24 58 48 8d 8f 48 02 00 00";
    /// The same in ATS 1.62.1.1.
    const SET_TARGETS_1_62: &str = "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f9 89 91 d8 01 00 00 48 81 c1 \
        e0 01 00 00 33 f6 49 8b d9 4c 3b c1 74 0c 49 8b d0 48 89 71 10 e8 2b 3c 00 00 48 8d 8f 08 02 00 00 48 3b d9 \
        74 0c 48 8b d3 48 89 71 10 e8 13 3c 00 00 48 8b 54 24 50 48 8d 8f 30 02 00 00 48 3b d1 74 09 48 89 71 10 e8 \
        f9 3b 00 00 48 8b 54 24 58 48 8d 8f 58 02 00 00";
    /// The "next node" getter in ETS2 1.61.1.9 and ATS 1.62.1.1.
    const NEXT_NODE_1_61: &str = "40 53 48 83 ec 20 48 8b 41 08 48 8b d9 48 83 c1 08 ff 50 30 3b 83 90 01 00 00 75 \
        0d 48 8b 83 98 01 00 00 48 83 c4 20 5b c3";
    const NEXT_NODE_1_62: &str = "40 53 48 83 ec 20 48 8b 41 08 48 8b d9 48 83 c1 08 ff 50 30 3b 83 a0 01 00 00 75 \
        0d 48 8b 83 a8 01 00 00 48 83 c4 20 5b c3";

    /// The walk back along a route's nodes in ETS2 1.61.1.9 and ATS 1.62.1.1.
    const ROUTE_WALK_1_61: &str = "4d 8b 6b 08 49 8d 5d d0 4c 3b e3 74 41 49 83 fa 02 73 3b 44 8b 09 45 85 c9 74 0c \
        41 83 f9 01 75 16 48 39 79 08 eb 0e 48 8b 41 08 48 39 78 28 74 51 48 39 78 20 74 4b 48 83 e9 30";
    const ROUTE_WALK_1_62: &str = "4d 8b 6b 08 49 8d 5d c8 4c 3b e3 74 41 49 83 fa 02 73 3b 44 8b 0a 45 85 c9 74 0c \
        41 83 f9 01 75 16 48 39 7a 08 eb 0e 48 8b 42 08 48 39 78 28 74 4a 48 39 78 20 74 44 48 83 ea 38";

    #[test]
    fn reads_a_route_nodes_size_in_each_version() {
        assert_eq!(read_route_walk(&bytes(ROUTE_WALK_1_61)), Some(0x30));
        assert_eq!(read_route_walk(&bytes(ROUTE_WALK_1_62)), Some(0x38));
        // The two places saying different sizes: not taken.
        let mut apart = bytes(ROUTE_WALK_1_62);
        apart[58] = 0x40;
        assert_eq!(read_route_walk(&apart), None);
    }

    fn bytes(hex: &str) -> Vec<u8> {
        hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn reads_set_targets_lists_in_each_version() {
        assert_eq!(
            read_set_targets(&bytes(SET_TARGETS_1_61)),
            Some(Targets { mode: 0x1c8, planned: 0x1d0, route: 0x1f8, avoid: 0x220, waypoints: 0x248 })
        );
        assert_eq!(
            read_set_targets(&bytes(SET_TARGETS_1_62)),
            Some(Targets { mode: 0x1d8, planned: 0x1e0, route: 0x208, avoid: 0x230, waypoints: 0x258 })
        );
    }

    #[test]
    fn reads_the_next_node_in_each_version() {
        assert_eq!(read_next_node(&bytes(NEXT_NODE_1_61)), Some(NextNode { id: 0x190, node: 0x198 }));
        assert_eq!(read_next_node(&bytes(NEXT_NODE_1_62)), Some(NextNode { id: 0x1a0, node: 0x1a8 }));
    }

    #[test]
    fn reads_nothing_that_does_not_add_up() {
        // A list one size off: not taken.
        let mut skewed = bytes(SET_TARGETS_1_62);
        let at = skewed.len() - 4;
        skewed[at] = 0x60;
        assert_eq!(read_set_targets(&skewed), None);
        // Another function's start.
        assert_eq!(read_set_targets(&bytes(NEXT_NODE_1_62)), None);
        let mut node_apart = bytes(NEXT_NODE_1_62);
        node_apart[31] = 0xb0;
        assert_eq!(read_next_node(&node_apart), None);
    }

    #[test]
    fn finds_a_shape_once_with_its_offsets_left_open() {
        let mut code = vec![0xcc; 16];
        code.extend(bytes(NEXT_NODE_1_62));
        assert_eq!(find_unique(&code, &NEXT_NODE), Some(16));
        code.extend(bytes(NEXT_NODE_1_61));
        assert_eq!(find_unique(&code, &NEXT_NODE), None);
    }
}
