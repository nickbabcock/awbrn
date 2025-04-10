use serde::{
    Deserialize, Deserializer, Serialize, de,
    de::value::{
        BoolDeserializer, BorrowedBytesDeserializer, BorrowedStrDeserializer, BytesDeserializer,
        CharDeserializer, EnumAccessDeserializer, F32Deserializer, F64Deserializer, I8Deserializer,
        I16Deserializer, I32Deserializer, I64Deserializer, I128Deserializer, MapAccessDeserializer,
        SeqAccessDeserializer, StrDeserializer, StringDeserializer, U8Deserializer,
        U16Deserializer, U32Deserializer, U64Deserializer, U128Deserializer, UnitDeserializer,
    },
};
use std::marker::PhantomData;

pub fn deserialize_vec_pair<'de, D, K, V>(deserializer: D) -> Result<Vec<(K, V)>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de>,
    V: Deserialize<'de>,
{
    struct VecPairVisitor<K1, V1> {
        marker: PhantomData<Vec<(K1, V1)>>,
    }

    impl<'de, K1, V1> de::Visitor<'de> for VecPairVisitor<K1, V1>
    where
        K1: Deserialize<'de>,
        V1: Deserialize<'de>,
    {
        type Value = Vec<(K1, V1)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map containing key value tuples")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some((key, value)) = map.next_entry()? {
                values.push((key, value));
            }

            Ok(values)
        }
    }

    deserializer.deserialize_map(VecPairVisitor {
        marker: PhantomData,
    })
}

/// Value is hidden with an empty string
#[derive(Debug, Serialize, Default)]
#[serde(untagged)]
pub enum Hidden<T> {
    #[default]
    Hidden,
    Visible(T),
}

impl<'de, T> Deserialize<'de> for Hidden<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HiddenVisitor<T> {
            marker: std::marker::PhantomData<T>,
        }

        impl<'de, T> serde::de::Visitor<'de> for HiddenVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Hidden<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an empty string or T")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.is_empty() {
                    Ok(Hidden::Hidden)
                } else {
                    T::deserialize(StrDeserializer::new(value)).map(Hidden::Visible)
                }
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.is_empty() {
                    Ok(Hidden::Hidden)
                } else {
                    T::deserialize(StringDeserializer::new(value)).map(Hidden::Visible)
                }
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.is_empty() {
                    Ok(Hidden::Hidden)
                } else {
                    T::deserialize(BorrowedStrDeserializer::new(value)).map(Hidden::Visible)
                }
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BoolDeserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I8Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I16Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I32Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I64Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I128Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U8Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U16Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U32Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U64Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U128Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(F32Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(F64Deserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(CharDeserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BytesDeserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BorrowedBytesDeserializer::new(v)).map(Hidden::Visible)
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_bytes(v.as_slice())
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(de::Error::invalid_type(de::Unexpected::Option, &self))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                T::deserialize(deserializer).map(Hidden::Visible)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(UnitDeserializer::new()).map(Hidden::Visible)
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                T::deserialize(deserializer).map(Hidden::Visible)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                T::deserialize(SeqAccessDeserializer::new(seq)).map(Hidden::Visible)
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                T::deserialize(MapAccessDeserializer::new(map)).map(Hidden::Visible)
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: de::EnumAccess<'de>,
            {
                T::deserialize(EnumAccessDeserializer::new(data)).map(Hidden::Visible)
            }
        }

        deserializer.deserialize_any(HiddenVisitor {
            marker: std::marker::PhantomData,
        })
    }
}

/// Value was masked with a "?"
#[derive(Debug, Serialize, Default)]
#[serde(untagged)]
pub enum Masked<T> {
    #[default]
    Masked,
    Visible(T),
}

impl<'de, T> Deserialize<'de> for Masked<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MaskedVisitor<T> {
            marker: std::marker::PhantomData<T>,
        }

        impl<'de, T> serde::de::Visitor<'de> for MaskedVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Masked<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or T")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == "?" {
                    Ok(Masked::Masked)
                } else {
                    T::deserialize(StrDeserializer::new(value)).map(Masked::Visible)
                }
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == "?" {
                    Ok(Masked::Masked)
                } else {
                    T::deserialize(StringDeserializer::new(value)).map(Masked::Visible)
                }
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value == "?" {
                    Ok(Masked::Masked)
                } else {
                    T::deserialize(BorrowedStrDeserializer::new(value)).map(Masked::Visible)
                }
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BoolDeserializer::new(v)).map(Masked::Visible)
            }

            fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I8Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I16Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I32Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I64Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(I128Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U8Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U16Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U32Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U64Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(U128Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(F32Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(F64Deserializer::new(v)).map(Masked::Visible)
            }

            fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(CharDeserializer::new(v)).map(Masked::Visible)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BytesDeserializer::new(v)).map(Masked::Visible)
            }

            fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(BorrowedBytesDeserializer::new(v)).map(Masked::Visible)
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_bytes(v.as_slice())
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(de::Error::invalid_type(de::Unexpected::Option, &self))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                T::deserialize(deserializer).map(Masked::Visible)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                T::deserialize(UnitDeserializer::new()).map(Masked::Visible)
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                T::deserialize(deserializer).map(Masked::Visible)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                T::deserialize(SeqAccessDeserializer::new(seq)).map(Masked::Visible)
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                T::deserialize(MapAccessDeserializer::new(map)).map(Masked::Visible)
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: de::EnumAccess<'de>,
            {
                T::deserialize(EnumAccessDeserializer::new(data)).map(Masked::Visible)
            }
        }

        deserializer.deserialize_any(MaskedVisitor {
            marker: std::marker::PhantomData,
        })
    }
}
