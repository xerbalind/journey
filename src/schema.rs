// @generated automatically by Diesel CLI.

diesel::table! {
    points (id) {
        id -> Int4,
        lon -> Float4,
        lat -> Float4,
        alt -> Nullable<Int2>,
        time -> Timestamp,
        processed -> Bool,
    }
}

diesel::table! {
    routes (id) {
        id -> Int4,
        point_id -> Nullable<Int4>,
        lon -> Float4,
        lat -> Float4,
        alt -> Nullable<Int2>,
        distance -> Nullable<Float8>,
    }
}

diesel::table! {
    temperatures (id) {
        id -> Int4,
        value -> Float4,
        time -> Timestamp,
    }
}

diesel::joinable!(routes -> points (point_id));

diesel::allow_tables_to_appear_in_same_query!(points, routes, temperatures,);
