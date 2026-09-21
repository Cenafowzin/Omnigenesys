use super::*;

#[test]
fn tile_id_costs_no_more_than_the_u16_it_wraps() {
    // PLANNING §8.5 sizes dense layers at 2 bytes per cell. The newtype exists
    // for type safety only; it must not add a byte over the wrapped u16.
    assert_eq!(size_of::<TileId>(), size_of::<u16>());
    assert_eq!(size_of::<TileId>(), 2);
}
