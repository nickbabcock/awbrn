use serde::{
    Deserialize, Deserializer, Serialize, de,
    de::value::{BorrowedStrDeserializer, StrDeserializer, StringDeserializer},
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

/// Helper trait for implementing special value deserializers
trait SpecialValueDeserializer<T> {
    /// Check if a string represents the special value
    fn is_special(value: &str) -> bool;

    /// Create the special variant
    fn create_special() -> Self;

    /// Create the visible variant
    fn create_visible(value: T) -> Self;
}

/// Value is hidden with an empty string
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum Hidden<T> {
    #[default]
    Hidden,
    Visible(T),
}

impl<T> SpecialValueDeserializer<T> for Hidden<T> {
    fn is_special(value: &str) -> bool {
        value.is_empty()
    }

    fn create_special() -> Self {
        Hidden::Hidden
    }

    fn create_visible(value: T) -> Self {
        Hidden::Visible(value)
    }
}

/// Value was masked with a "?"
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum Masked<T> {
    #[default]
    Masked,
    Visible(T),
}

impl<T> SpecialValueDeserializer<T> for Masked<T> {
    fn is_special(value: &str) -> bool {
        value == "?"
    }

    fn create_special() -> Self {
        Masked::Masked
    }

    fn create_visible(value: T) -> Self {
        Masked::Visible(value)
    }
}

impl<T> Serialize for Hidden<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Hidden::Hidden => serializer.serialize_str(""),
            Hidden::Visible(value) => value.serialize(serializer),
        }
    }
}

impl<T> Serialize for Masked<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Masked::Masked => serializer.serialize_str("?"),
            Masked::Visible(value) => value.serialize(serializer),
        }
    }
}

fn deserialize_special_value<'de, D, T, S>(deserializer: D) -> Result<S, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    S: SpecialValueDeserializer<T>,
{
    struct SpecialVisitor<T, S> {
        marker: PhantomData<(T, S)>,
    }

    impl<'de, T, S> de::Visitor<'de> for SpecialVisitor<T, S>
    where
        T: Deserialize<'de>,
        S: SpecialValueDeserializer<T>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or a value")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if S::is_special(value) {
                Ok(S::create_special())
            } else {
                T::deserialize(StrDeserializer::new(value)).map(S::create_visible)
            }
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if S::is_special(&value) {
                Ok(S::create_special())
            } else {
                T::deserialize(StringDeserializer::new(value)).map(S::create_visible)
            }
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if S::is_special(value) {
                Ok(S::create_special())
            } else {
                T::deserialize(BorrowedStrDeserializer::new(value)).map(S::create_visible)
            }
        }

        // For all non-string types, just deserialize directly to visible variant
        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::BoolDeserializer::new(v),
            )?))
        }

        fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::I8Deserializer::new(v),
            )?))
        }

        fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::I16Deserializer::new(v),
            )?))
        }

        fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::I32Deserializer::new(v),
            )?))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::I64Deserializer::new(v),
            )?))
        }

        fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::U8Deserializer::new(v),
            )?))
        }

        fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::U16Deserializer::new(v),
            )?))
        }

        fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::U32Deserializer::new(v),
            )?))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::U64Deserializer::new(v),
            )?))
        }

        fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::F32Deserializer::new(v),
            )?))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::F64Deserializer::new(v),
            )?))
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::SeqAccessDeserializer::new(seq),
            )?))
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            Ok(S::create_visible(T::deserialize(
                serde::de::value::MapAccessDeserializer::new(map),
            )?))
        }
    }

    deserializer.deserialize_any(SpecialVisitor {
        marker: PhantomData,
    })
}

impl<'de, T> Deserialize<'de> for Hidden<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_special_value(deserializer)
    }
}

impl<'de, T> Deserialize<'de> for Masked<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_special_value(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hidden_roundtrip() {
        // Test Hidden::Hidden
        let hidden: Hidden<String> = Hidden::Hidden;
        let serialized = serde_json::to_string(&hidden).unwrap();
        assert_eq!(serialized, r#""""#); // Should serialize as empty string

        let deserialized: Hidden<String> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Hidden);

        // Test Hidden::Visible
        let visible = Hidden::Visible("test".to_string());
        let serialized = serde_json::to_string(&visible).unwrap();
        assert_eq!(serialized, r#""test""#); // Should serialize the inner value

        let deserialized: Hidden<String> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Visible("test".to_string()));
    }

    #[test]
    fn test_masked_roundtrip() {
        // Test Masked::Masked
        let masked: Masked<String> = Masked::Masked;
        let serialized = serde_json::to_string(&masked).unwrap();
        assert_eq!(serialized, r#""?""#); // Should serialize as "?"

        let deserialized: Masked<String> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Masked::Masked);

        // Test Masked::Visible
        let visible = Masked::Visible("test".to_string());
        let serialized = serde_json::to_string(&visible).unwrap();
        assert_eq!(serialized, r#""test""#); // Should serialize the inner value

        let deserialized: Masked<String> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Masked::Visible("test".to_string()));
    }

    #[test]
    fn test_hidden_with_different_types() {
        // Test with numeric type
        let hidden_num: Hidden<i32> = Hidden::Hidden;
        let serialized = serde_json::to_string(&hidden_num).unwrap();
        assert_eq!(serialized, r#""""#);

        let visible_num = Hidden::Visible(42);
        let serialized = serde_json::to_string(&visible_num).unwrap();
        assert_eq!(serialized, r#"42"#);

        let deserialized: Hidden<i32> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Visible(42));

        // Test with boolean type
        let visible_bool = Hidden::Visible(true);
        let serialized = serde_json::to_string(&visible_bool).unwrap();
        assert_eq!(serialized, r#"true"#);

        let deserialized: Hidden<bool> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Visible(true));
    }

    #[test]
    fn test_masked_with_different_types() {
        // Test with numeric type
        let masked_num: Masked<i32> = Masked::Masked;
        let serialized = serde_json::to_string(&masked_num).unwrap();
        assert_eq!(serialized, r#""?""#);

        let visible_num = Masked::Visible(42);
        let serialized = serde_json::to_string(&visible_num).unwrap();
        assert_eq!(serialized, r#"42"#);

        let deserialized: Masked<i32> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Masked::Visible(42));

        // Test with boolean type
        let visible_bool = Masked::Visible(true);
        let serialized = serde_json::to_string(&visible_bool).unwrap();
        assert_eq!(serialized, r#"true"#);

        let deserialized: Masked<bool> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Masked::Visible(true));
    }

    #[test]
    fn test_hidden_with_complex_types() {
        // Test with Vec
        let visible_vec = Hidden::Visible(vec![1, 2, 3]);
        let serialized = serde_json::to_string(&visible_vec).unwrap();
        assert_eq!(serialized, r#"[1,2,3]"#);

        let deserialized: Hidden<Vec<i32>> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Visible(vec![1, 2, 3]));
    }

    #[test]
    fn test_masked_with_complex_types() {
        // Test with Vec
        let visible_vec = Masked::Visible(vec![1, 2, 3]);
        let serialized = serde_json::to_string(&visible_vec).unwrap();
        assert_eq!(serialized, r#"[1,2,3]"#);

        let deserialized: Masked<Vec<i32>> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Masked::Visible(vec![1, 2, 3]));
    }

    #[test]
    fn test_hidden_with_struct() {
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct TestStruct {
            id: u32,
            name: String,
        }

        let test_struct = TestStruct {
            id: 42,
            name: "test".to_string(),
        };
        let visible_struct = Hidden::Visible(test_struct);
        let serialized = serde_json::to_string(&visible_struct).unwrap();

        let expected = r#"{"id":42,"name":"test"}"#;
        assert_eq!(serialized, expected);

        let deserialized: Hidden<TestStruct> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            deserialized,
            Hidden::Visible(TestStruct {
                id: 42,
                name: "test".to_string()
            })
        );

        // Test with a hidden struct
        let hidden_struct: Hidden<TestStruct> = Hidden::Hidden;
        let serialized = serde_json::to_string(&hidden_struct).unwrap();
        assert_eq!(serialized, r#""""#);

        let deserialized: Hidden<TestStruct> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Hidden::Hidden);
    }
}
