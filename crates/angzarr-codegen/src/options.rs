//! The codegen options a descriptor set declares (`CODEGEN_PROTO`,
//! `io.angzarr.codegen.v1`): each read through prost-reflect from the
//! options as protoc wrote them, and its default where the set never
//! imported the file or the element declares none.

use prost_reflect::{DescriptorPool, DynamicMessage, FieldDescriptor, MessageDescriptor, Value};

/// The options package's extensions, by the options message they extend.
const FIELD: &str = "io.angzarr.codegen.v1.field";
const MESSAGE: &str = "io.angzarr.codegen.v1.message";

/// The value of extension `name` in `options`, when the pool declares it
/// and the options set it.
fn declared(pool: &DescriptorPool, options: &DynamicMessage, name: &str) -> Option<DynamicMessage> {
    let ext = pool.get_extension_by_name(name)?;
    if !options.has_extension(&ext) {
        return None;
    }
    match options.get_extension(&ext).into_owned() {
        Value::Message(m) => Some(m),
        _ => None,
    }
}

fn string(message: Option<DynamicMessage>, field: &str) -> String {
    match message.and_then(|m| m.get_field_by_name(field).map(|v| v.into_owned())) {
        Some(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn flag(message: Option<DynamicMessage>, field: &str) -> bool {
    matches!(
        message.and_then(|m| m.get_field_by_name(field).map(|v| v.into_owned())),
        Some(Value::Bool(true))
    )
}

fn field(f: &FieldDescriptor) -> Option<DynamicMessage> {
    declared(f.parent_pool(), &f.options(), FIELD)
}

fn message(m: &MessageDescriptor) -> Option<DynamicMessage> {
    declared(m.parent_pool(), &m.options(), MESSAGE)
}

/// Whether the component anchor `m` is its component's state
/// (`MessageCodegen.state`).
pub(crate) fn is_state(m: &MessageDescriptor) -> bool {
    flag(message(m), "state")
}

/// The id newtype a UUID field declares (`FieldCodegen.id_type`), if any.
pub(crate) fn id_type(f: &FieldDescriptor) -> Option<String> {
    Some(string(field(f), "id_type")).filter(|s| !s.is_empty())
}
