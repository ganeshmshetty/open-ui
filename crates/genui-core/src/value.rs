//! Semantic values. JSON is a wire format, never the evaluator's value model (§5.5).
use crate::syntax::ast::{Expr, UnaryOp};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Provenance {
    #[default]
    Model,
    Host,
    Tool,
    User,
    Mixed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PStr {
    pub text: Arc<str>,
    pub prov: Provenance,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(PStr),
    List(Arc<Vec<Value>>),
    Map(Arc<Vec<(String, Value)>>),
    Ref(String),
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Str(PStr {
            text: s.into(),
            prov: Provenance::Model,
        })
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(v.into())
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}
impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.plain_json().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a finite semantic value")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
                Ok(Value::Int(v))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
                i64::try_from(v).map(Value::Int).map_err(E::custom)
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
                if v.is_finite() {
                    Ok(Value::Float(v))
                } else {
                    Err(E::custom("non-finite value"))
                }
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
                Ok(Value::from(v))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
                Ok(Value::from(v))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Value, A::Error> {
                let mut v = Vec::new();
                while let Some(item) = seq.next_element()? {
                    v.push(item)
                }
                Ok(Value::List(Arc::new(v)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Value, A::Error> {
                let mut v = Vec::new();
                while let Some(item) = map.next_entry()? {
                    v.push(item)
                }
                Ok(Value::Map(Arc::new(v)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
impl Value {
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Float(f) => f.is_finite(),
            Self::List(v) => v.iter().all(Self::is_finite),
            Self::Map(v) => v.iter().all(|(_, v)| v.is_finite()),
            _ => true,
        }
    }
    pub fn to_expr(&self) -> Expr {
        match self {
            Self::Null => Expr::null(),
            Self::Bool(value) => Expr::Bool { value: *value },
            Self::Int(value) => Expr::Int { value: *value },
            Self::Float(value) => Expr::Float { value: *value },
            Self::Str(s) => Expr::String {
                value: s.text.to_string(),
            },
            Self::List(v) => Expr::List {
                items: v.iter().map(Self::to_expr).collect(),
            },
            Self::Map(v) => Expr::Map {
                entries: v
                    .iter()
                    .map(|(key, v)| crate::syntax::ast::Entry {
                        key: key.clone(),
                        value: v.to_expr(),
                    })
                    .collect(),
            },
            Self::Ref(id) => Expr::Call {
                name: "ref".into(),
                args: vec![crate::syntax::ast::Argument {
                    name: None,
                    value: Expr::String { value: id.clone() },
                }],
            },
        }
    }
    pub fn string(&self) -> Option<&str> {
        if let Self::Str(s) = self {
            Some(&s.text)
        } else {
            None
        }
    }
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn plain_json(&self) -> serde_json::Value {
        self.json(false)
    }
    pub fn ir_json(&self) -> serde_json::Value {
        self.json(true)
    }
    fn json(&self, typed: bool) -> serde_json::Value {
        use serde_json::{Value as J, json};
        match self {
            Self::Null => J::Null,
            Self::Bool(b) => json!(b),
            Self::Int(i) => {
                if typed {
                    json!({"i":i})
                } else {
                    json!(i)
                }
            }
            Self::Float(f) => {
                if typed {
                    json!({"f":f})
                } else {
                    json!(f)
                }
            }
            Self::Str(s) => json!(s.text.as_ref()),
            Self::Ref(s) => json!({"ref":s}),
            Self::List(v) => J::Array(v.iter().map(|v| v.json(typed)).collect()),
            Self::Map(v) if typed => {
                json!({"map":v.iter().map(|(k,v)|json!([k,v.json(true)])).collect::<Vec<_>>()})
            }
            Self::Map(v) => J::Object(v.iter().map(|(k, v)| (k.clone(), v.json(false))).collect()),
        }
    }
    /// Constants are syntax literals and containers, including signed numeric literals.
    pub fn constant(e: &Expr) -> Option<Self> {
        Self::constant_inner(e, false)
    }
    pub fn constant_recovered(e: &Expr) -> Option<Self> {
        Self::constant_inner(e, true)
    }
    fn constant_inner(e: &Expr, recover: bool) -> Option<Self> {
        Some(match e {
            Expr::Null { origin } => {
                if !origin.recovered || recover {
                    Self::Null
                } else {
                    return None;
                }
            }
            Expr::Bool { value } => Self::Bool(*value),
            Expr::Int { value } => Self::Int(*value),
            Expr::Float { value } => Self::Float(*value),
            Expr::String { value } => Self::from(value.clone()),
            Expr::List { items } => Self::List(Arc::new(
                items
                    .iter()
                    .map(|e| Self::constant_inner(e, recover))
                    .collect::<Option<_>>()?,
            )),
            Expr::Map { entries } => Self::Map(Arc::new(
                entries
                    .iter()
                    .map(|e| Some((e.key.clone(), Self::constant_inner(&e.value, recover)?)))
                    .collect::<Option<_>>()?,
            )),
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
            } => match Self::constant_inner(expr, recover)? {
                Self::Int(i) => Self::Int(i.checked_neg()?),
                Self::Float(f) => Self::Float(-f),
                _ => return None,
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_deserialization_keeps_insertion_order() {
        let v: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        assert!(matches!(v,Value::Map(m)if m[0].0=="z"&&m[1].0=="a"));
    }
    #[test]
    fn cloned_containers_share_storage() {
        let a = Value::List(Arc::new(vec![Value::from("large")]));
        let b = a.clone();
        assert!(matches!((a,b),(Value::List(a),Value::List(b))if Arc::ptr_eq(&a,&b)));
    }
    #[test]
    fn wire_values_keep_number_kinds() {
        let a = Value::Int(2);
        let b = Value::Float(2.0);
        assert_ne!(a.ir_json(), b.ir_json());
        assert_eq!(a.ir_json(), serde_json::json!({"i":2}));
        assert_eq!(b.ir_json(), serde_json::json!({"f":2.0}));
    }
}
