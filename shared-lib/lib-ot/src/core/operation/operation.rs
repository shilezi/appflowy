use crate::core::{Attribute, Attributes, FlowyStr, Interval, OpBuilder};
use serde::__private::Formatter;
use std::{
    cmp::min,
    fmt,
    ops::{Deref, DerefMut},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Operation {
    Delete(usize),
    Retain(Retain),
    Insert(Insert),
}

impl Operation {
    pub fn get_data(&self) -> &str {
        match self {
            Operation::Delete(_) => "",
            Operation::Retain(_) => "",
            Operation::Insert(insert) => &insert.s,
        }
    }

    pub fn get_attributes(&self) -> Attributes {
        match self {
            Operation::Delete(_) => Attributes::default(),
            Operation::Retain(retain) => retain.attributes.clone(),
            Operation::Insert(insert) => insert.attributes.clone(),
        }
    }

    pub fn set_attributes(&mut self, attributes: Attributes) {
        match self {
            Operation::Delete(_) => log::error!("Delete should not contains attributes"),
            Operation::Retain(retain) => retain.attributes = attributes,
            Operation::Insert(insert) => insert.attributes = attributes,
        }
    }

    pub fn has_attribute(&self) -> bool { !self.get_attributes().is_empty() }

    pub fn contain_attribute(&self, attribute: &Attribute) -> bool {
        self.get_attributes().contains_key(&attribute.key)
    }

    pub fn len(&self) -> usize {
        match self {
            Operation::Delete(n) => *n,
            Operation::Retain(r) => r.n,
            Operation::Insert(i) => i.count_of_code_units(),
        }
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    #[allow(dead_code)]
    pub fn split(&self, index: usize) -> (Option<Operation>, Option<Operation>) {
        debug_assert!(index < self.len());
        let left;
        let right;
        match self {
            Operation::Delete(n) => {
                left = Some(OpBuilder::delete(index).build());
                right = Some(OpBuilder::delete(*n - index).build());
            },
            Operation::Retain(retain) => {
                left = Some(OpBuilder::delete(index).build());
                right = Some(OpBuilder::delete(retain.n - index).build());
            },
            Operation::Insert(insert) => {
                let attributes = self.get_attributes();
                left = Some(
                    OpBuilder::insert(&insert.s[0..index])
                        .attributes(attributes.clone())
                        .build(),
                );
                right = Some(
                    OpBuilder::insert(&insert.s[index..insert.count_of_code_units()])
                        .attributes(attributes)
                        .build(),
                );
            },
        }

        (left, right)
    }

    pub fn shrink(&self, interval: Interval) -> Option<Operation> {
        let op = match self {
            Operation::Delete(n) => OpBuilder::delete(min(*n, interval.size())).build(),
            Operation::Retain(retain) => OpBuilder::retain(min(retain.n, interval.size()))
                .attributes(retain.attributes.clone())
                .build(),
            Operation::Insert(insert) => {
                if interval.start > insert.count_of_code_units() {
                    OpBuilder::insert("").build()
                } else {
                    // let s = &insert
                    //     .s
                    //     .chars()
                    //     .skip(interval.start)
                    //     .take(min(interval.size(), insert.count_of_code_units()))
                    //     .collect::<String>();

                    let s = insert.s.sub_str(interval);
                    OpBuilder::insert(&s).attributes(insert.attributes.clone()).build()
                }
            },
        };

        match op.is_empty() {
            true => None,
            false => Some(op),
        }
    }

    pub fn is_delete(&self) -> bool {
        if let Operation::Delete(_) = self {
            return true;
        }
        false
    }

    pub fn is_insert(&self) -> bool {
        if let Operation::Insert(_) = self {
            return true;
        }
        false
    }

    pub fn is_retain(&self) -> bool {
        if let Operation::Retain(_) = self {
            return true;
        }
        false
    }

    pub fn is_plain(&self) -> bool {
        match self {
            Operation::Delete(_) => true,
            Operation::Retain(retain) => retain.is_plain(),
            Operation::Insert(insert) => insert.is_plain(),
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        match self {
            Operation::Delete(n) => {
                f.write_fmt(format_args!("delete: {}", n))?;
            },
            Operation::Retain(r) => {
                f.write_fmt(format_args!("{}", r))?;
            },
            Operation::Insert(i) => {
                f.write_fmt(format_args!("{}", i))?;
            },
        }
        f.write_str("}")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Retain {
    #[serde(rename(serialize = "retain", deserialize = "retain"))]
    pub n: usize,
    #[serde(skip_serializing_if = "is_empty")]
    pub attributes: Attributes,
}

impl fmt::Display for Retain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.attributes.is_empty() {
            f.write_fmt(format_args!("retain: {}", self.n))
        } else {
            f.write_fmt(format_args!("retain: {}, attributes: {}", self.n, self.attributes))
        }
    }
}

impl Retain {
    pub fn merge_or_new(&mut self, n: usize, attributes: Attributes) -> Option<Operation> {
        tracing::trace!(
            "merge_retain_or_new_op: len: {:?}, l: {} - r: {}",
            n,
            self.attributes,
            attributes
        );
        if self.attributes == attributes {
            self.n += n;
            None
        } else {
            Some(OpBuilder::retain(n).attributes(attributes).build())
        }
    }

    pub fn is_plain(&self) -> bool { self.attributes.is_empty() }
}

impl std::convert::From<usize> for Retain {
    fn from(n: usize) -> Self {
        Retain {
            n,
            attributes: Attributes::default(),
        }
    }
}

impl Deref for Retain {
    type Target = usize;

    fn deref(&self) -> &Self::Target { &self.n }
}

impl DerefMut for Retain {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.n }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Insert {
    #[serde(rename(serialize = "insert", deserialize = "insert"))]
    pub s: FlowyStr,

    #[serde(skip_serializing_if = "is_empty")]
    pub attributes: Attributes,
}

impl fmt::Display for Insert {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut s = self.s.clone();
        if s.ends_with('\n') {
            s.pop();
            if s.is_empty() {
                s = "new_line".into();
            }
        }

        if self.attributes.is_empty() {
            f.write_fmt(format_args!("insert: {}", s))
        } else {
            f.write_fmt(format_args!("insert: {}, attributes: {}", s, self.attributes))
        }
    }
}

impl Insert {
    pub fn count_of_code_units(&self) -> usize { self.s.count_utf16_code_units() }

    pub fn merge_or_new_op(&mut self, s: &str, attributes: Attributes) -> Option<Operation> {
        if self.attributes == attributes {
            self.s += s;
            None
        } else {
            Some(OpBuilder::insert(s).attributes(attributes).build())
        }
    }

    pub fn is_plain(&self) -> bool { self.attributes.is_empty() }
}

impl std::convert::From<String> for Insert {
    fn from(s: String) -> Self {
        Insert {
            s: s.into(),
            attributes: Attributes::default(),
        }
    }
}

impl std::convert::From<&str> for Insert {
    fn from(s: &str) -> Self { Insert::from(s.to_owned()) }
}

impl std::convert::From<FlowyStr> for Insert {
    fn from(s: FlowyStr) -> Self {
        Insert {
            s,
            attributes: Attributes::default(),
        }
    }
}

fn is_empty(attributes: &Attributes) -> bool { attributes.is_empty() }
