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

diesel::table! {
    temperatures (id) {
        id -> Int4,
        value -> Float4,
        time -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(points, temperatures,);
