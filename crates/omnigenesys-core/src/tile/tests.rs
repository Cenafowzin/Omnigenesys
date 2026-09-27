use super::*;

#[test]
fn tile_id_costs_no_more_than_the_u16_it_wraps() {
    // PLANNING §8.5 sizes dense layers at 2 bytes per cell. The newtype exists
    // for type safety only; it must not add a byte over the wrapped u16.
    assert_eq!(size_of::<TileId>(), size_of::<u16>());
    assert_eq!(size_of::<TileId>(), 2);
}

// TileId::EMPTY

#[test]
fn empty_is_tile_zero() {
    // Output contract (PLANNING 2.4): index 0 of the string table is "empty".
    assert_eq!(TileId::EMPTY, TileId(0));
}

// TileRegistry

#[test]
fn empty_is_always_registered_at_zero() {
    let reg = TileRegistry::new(["floor", "wall"]).unwrap();
    assert_eq!(reg.id("empty"), Ok(TileId::EMPTY));
    assert_eq!(reg.name(TileId::EMPTY), Some("empty"));
}

#[test]
fn declared_tiles_follow_declaration_order() {
    // Deliberately not alphabetical: ids follow the declaration, not a sort.
    let reg = TileRegistry::new(["wall", "floor", "water"]).unwrap();
    assert_eq!(reg.id("wall"), Ok(TileId(1)));
    assert_eq!(reg.id("floor"), Ok(TileId(2)));
    assert_eq!(reg.id("water"), Ok(TileId(3)));
}

#[test]
fn id_and_name_round_trip() {
    let reg = TileRegistry::new(["wall", "floor", "water"]).unwrap();
    for name in ["empty", "wall", "floor", "water"] {
        let id = reg.id(name).unwrap();
        assert_eq!(reg.name(id), Some(name));
    }
}

#[test]
fn an_empty_declaration_is_valid() {
    let reg = TileRegistry::new(Vec::<&str>::new()).unwrap();
    assert_eq!(reg.len(), 1);
    assert!(!reg.is_empty());
}

#[test]
fn len_counts_empty_too() {
    let reg = TileRegistry::new(["floor", "wall"]).unwrap();
    assert_eq!(reg.len(), 3);
}

#[test]
fn new_accepts_str_and_string() {
    let from_str = TileRegistry::new(["floor"]).unwrap();
    let from_string = TileRegistry::new(vec![String::from("floor")]).unwrap();
    assert_eq!(from_str, from_string);
}

#[test]
fn declaring_empty_is_rejected_as_reserved() {
    let err = TileRegistry::new(["floor", "empty"]).unwrap_err();
    assert_eq!(
        err,
        TileError::ReservedName {
            name: "empty".to_string()
        }
    );
}

#[test]
fn declaring_a_tile_twice_is_rejected() {
    let err = TileRegistry::new(["floor", "wall", "floor"]).unwrap_err();
    assert_eq!(
        err,
        TileError::DuplicateTile {
            name: "floor".to_string()
        }
    );
}

#[test]
fn an_undeclared_name_is_an_error_not_a_new_tile() {
    // The whole point of the declared list: a typo must fail (PLANNING 8.8.3).
    let reg = TileRegistry::new(["floor"]).unwrap();
    assert_eq!(
        reg.id("flor"),
        Err(TileError::UnknownTile {
            name: "flor".to_string()
        })
    );
    assert_eq!(reg.len(), 2);
}

#[test]
fn name_of_an_unregistered_id_is_none() {
    let reg = TileRegistry::new(["floor"]).unwrap();
    assert_eq!(reg.name(TileId(99)), None);
}

#[test]
fn registry_holds_exactly_u16_max_declared_tiles() {
    // "empty" takes position 0, so 65535 declared tiles fill ids 1..=65535.
    let names: Vec<String> = (0..u16::MAX).map(|i| format!("t{i}")).collect();
    let reg = TileRegistry::new(names).unwrap();
    assert_eq!(reg.id("t65534"), Ok(TileId(u16::MAX)));
}

#[test]
fn one_tile_past_u16_max_is_rejected() {
    let names: Vec<String> = (0..=u16::MAX).map(|i| format!("t{i}")).collect();
    let err = TileRegistry::new(names).unwrap_err();
    assert_eq!(err, TileError::TooManyTiles { count: 65536 });
}

#[test]
fn tile_error_messages() {
    let cases = [
        (
            TileError::ReservedName {
                name: "empty".to_string(),
            },
            "'empty' is reserved and cannot be declared",
        ),
        (
            TileError::DuplicateTile {
                name: "floor".to_string(),
            },
            "tile 'floor' is a duplicate",
        ),
        (
            TileError::UnknownTile {
                name: "flor".to_string(),
            },
            "tile 'flor' is unknown",
        ),
        (
            TileError::TooManyTiles { count: 65536 },
            "too many tiles registered: 65536",
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(err.to_string(), expected);
    }
}
