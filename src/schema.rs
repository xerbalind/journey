// @generated automatically by Diesel CLI.

diesel::table! {
    points (id) {
        id -> Int4,
        lon -> Float4,
        lat -> Float4,
        alt -> Nullable<Int2>,
        time -> Timestamp,
    }
}
