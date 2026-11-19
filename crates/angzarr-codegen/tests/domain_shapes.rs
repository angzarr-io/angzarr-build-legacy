//! The domain layer of every shape beyond scalars, lists and maps (the
//! `shapes` fixtures): another package's types, referenced through its own
//! layer; angzarr's `UUID` and its id newtypes; `google.protobuf.Duration`;
//! oneofs and nested types; `Default` where every field has one; and a
//! component whose anchor is its state. Read back as Rust items; whether it
//! compiles and round-trips is angzarr-build's `tests/local` shapes-app.

mod common;

use angzarr_codegen::{
    CodegenError, Generated, ProtoError, UseError, adapters, markup_domain, markup_domain_with,
    scaffold,
};
use common::*;

const SHAPES: &[&str] = &[
    "shapes/common/v1/common.proto",
    "shapes/catalog/v1/catalog.proto",
    "shapes/order/v1/order.proto",
];
const WIRE: &[(&str, &str)] = &[
    ("shapes.common.v1", "crate::wire::shapes::common::v1"),
    ("shapes.catalog.v1", "crate::wire::shapes::catalog::v1"),
    ("shapes.order.v1", "crate::wire::shapes::order::v1"),
];
const DOMAIN: &[(&str, &str)] = &[
    ("shapes.common.v1", "crate::domain::common"),
    ("shapes.catalog.v1", "crate::domain::catalog"),
    ("shapes.order.v1", "crate::domain::order"),
];

fn layers() -> Vec<Generated> {
    markup_domain_with(&descriptors(SHAPES), SHAPES, WIRE, DOMAIN).unwrap()
}

fn source(package: &str) -> String {
    let layers = layers();
    layers
        .iter()
        .find(|g| g.package == package)
        .unwrap_or_else(|| panic!("no layer {package}: {layers:#?}"))
        .source
        .clone()
}

fn layer(package: &str) -> syn::File {
    syn::parse_file(&source(package)).expect("generated source parses")
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

fn item_struct<'a>(file: &'a syn::File, name: &str) -> Option<&'a syn::ItemStruct> {
    file.items.iter().find_map(|i| match i {
        syn::Item::Struct(s) if s.ident == name => Some(s),
        _ => None,
    })
}

fn fields(file: &syn::File, name: &str) -> Vec<(String, String)> {
    item_struct(file, name)
        .unwrap_or_else(|| panic!("no struct {name}"))
        .fields
        .iter()
        .map(|f| {
            let name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            (name, text(&f.ty))
        })
        .collect()
}

fn variants(file: &syn::File, name: &str) -> Vec<(String, String)> {
    file.items
        .iter()
        .find_map(|i| match i {
            syn::Item::Enum(e) if e.ident == name => Some(e),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no enum {name}"))
        .variants
        .iter()
        .map(|v| (v.ident.to_string(), text(&v.fields)))
        .collect()
}

/// `(trait, type)` of every impl in the file.
fn impls(file: &syn::File) -> Vec<(String, String)> {
    file.items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Impl(imp) => Some((
                imp.trait_
                    .as_ref()
                    .map(|(path, _)| text(path))
                    .unwrap_or_default(),
                text(&imp.self_ty),
            )),
            _ => None,
        })
        .collect()
}

/// The derives of the struct or enum `name`, as text.
fn derives(file: &syn::File, name: &str) -> String {
    let attrs = file
        .items
        .iter()
        .find_map(|i| match i {
            syn::Item::Struct(s) if s.ident == name => Some(&s.attrs),
            syn::Item::Enum(e) if e.ident == name => Some(&e.attrs),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no type {name}"));
    attrs
        .iter()
        .filter(|a| a.path().is_ident("derive"))
        .map(text)
        .collect()
}

/// The names of the file's structs, enums and traits.
fn names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Struct(s) => Some(s.ident.to_string()),
            syn::Item::Enum(e) => Some(e.ident.to_string()),
            syn::Item::Trait(t) => Some(t.ident.to_string()),
            _ => None,
        })
        .collect()
}

fn flat(source: &str) -> String {
    source.split_whitespace().collect()
}

// region another package's types

/// Each package's types are in its own layer, once: a package of types
/// alone (no component) gets a layer of them.
#[test]
fn test_each_package_has_its_own_layer_and_its_own_types() {
    let mut packages: Vec<String> = layers().into_iter().map(|g| g.package).collect();
    packages.sort();
    assert_eq!(
        packages,
        ["shapes.catalog.v1", "shapes.common.v1", "shapes.order.v1"]
    );
    assert!(item_struct(&layer("shapes.common.v1"), "Money").is_some());
    let order = layer("shapes.order.v1");
    for foreign in ["Item", "Money"] {
        assert!(
            item_struct(&order, foreign).is_none(),
            "{foreign} is another package's, not duplicated: {:?}",
            names(&order)
        );
    }
}

/// A field of another package's type is its own layer's domain type,
/// converted to and from that package's prost type.
#[test]
fn test_another_packages_type_is_referenced_through_its_layer() {
    let order = layer("shapes.order.v1");
    assert!(
        fields(&order, "PlaceOrder").contains(&(
            "items".to_string(),
            "Vec<crate::domain::catalog::Item>".to_string()
        )),
        "{:?}",
        fields(&order, "PlaceOrder")
    );
    let catalog = layer("shapes.catalog.v1");
    assert!(
        fields(&catalog, "Item").contains(&(
            "price".to_string(),
            "crate::domain::common::Money".to_string()
        )),
        "{:?}",
        fields(&catalog, "Item")
    );
    let source = source("shapes.order.v1");
    for expected in [
        "crate::wire::shapes::catalog::v1::Item::from",
        "crate::domain::catalog::Item::try_from",
    ] {
        assert!(
            flat(&source).contains(expected),
            "no {expected} in:\n{source}"
        );
    }
}

/// A package whose types another's reach, without a path for its domain
/// layer, is refused: nothing is duplicated into the package reaching it.
#[test]
fn test_another_packages_type_without_a_domain_path_is_refused() {
    assert_eq!(
        markup_domain(&descriptors(SHAPES), SHAPES, WIRE),
        Err(CodegenError::Use(UseError::NoDomainPath {
            package: "shapes.common.v1".to_string(),
            message: "shapes.catalog.v1.Item".to_string(),
        }))
    );
}

// endregion another package's types

// region UUID

/// angzarr's `UUID` is angzarr-proto's own type on both sides: required
/// when singular, a `Vec` when repeated; no domain type is generated for
/// it.
#[test]
fn test_a_uuid_is_angzarrs_own_type() {
    let order = layer("shapes.order.v1");
    let place = fields(&order, "PlaceOrder");
    for (field, ty) in [
        ("order", "::angzarr_embedded::v1::Uuid"),
        ("related", "Vec<::angzarr_embedded::v1::Uuid>"),
    ] {
        assert!(
            place.contains(&(field.to_string(), ty.to_string())),
            "{field}: {place:?}"
        );
    }
    assert!(!names(&order).iter().any(|n| n == "UUID" || n == "Uuid"));
    let source = source("shapes.order.v1");
    assert!(
        flat(&source)
            .contains("WireError::Missing{message:\"shapes.order.v1.PlaceOrder\",field:\"order\""),
        "{source}"
    );
}

/// A UUID field declaring `id_type` holds that newtype, declared once per
/// package with its conversions and its text.
#[test]
fn test_a_uuid_with_an_id_type_is_its_newtype() {
    let order = layer("shapes.order.v1");
    assert!(
        fields(&order, "PlaceOrder")
            .contains(&("customer".to_string(), "Option<CustomerId>".to_string()))
    );
    assert_eq!(
        fields(&order, "CustomerId"),
        pairs(&[("", "::angzarr_embedded::v1::Uuid")])
    );
    let count = names(&order).iter().filter(|n| *n == "CustomerId").count();
    assert_eq!(count, 1, "declared once: {:?}", names(&order));
    let impls = impls(&order);
    for (t, ty) in [
        ("From<::angzarr_embedded::v1::Uuid>", "CustomerId"),
        ("From<CustomerId>", "::angzarr_embedded::v1::Uuid"),
        ("::std::fmt::Display", "CustomerId"),
        ("::std::str::FromStr", "CustomerId"),
    ] {
        assert!(
            impls.contains(&(t.to_string(), ty.to_string())),
            "{t} for {ty}: {impls:?}"
        );
    }
    let catalog = layer("shapes.catalog.v1");
    assert!(
        fields(&catalog, "ListItem").contains(&("listing".to_string(), "ListingId".to_string()))
    );
}

// endregion UUID

/// `google.protobuf.Duration` is prost's type on both sides, required when
/// singular, as `Timestamp` is angzarr-embedded's.
#[test]
fn test_a_duration_is_prosts_type() {
    let place = fields(&layer("shapes.order.v1"), "PlaceOrder");
    for (field, ty) in [
        ("wait", "::prost_types::Duration"),
        ("grace", "Option<::prost_types::Duration>"),
    ] {
        assert!(
            place.contains(&(field.to_string(), ty.to_string())),
            "{field}: {place:?}"
        );
    }
}

// region oneofs and nested types

/// A oneof is one field of a generated choice, named after the message and
/// the oneof, one variant per arm holding the arm's domain type, in the
/// place of its first arm.
#[test]
fn test_a_oneof_is_a_choice_of_its_arms() {
    let order = layer("shapes.order.v1");
    let place = fields(&order, "PlaceOrder");
    let names: Vec<&str> = place.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "items", "order", "customer", "related", "wait", "grace", "payment", "pace"
        ]
    );
    assert!(place.contains(&("payment".to_string(), "PlaceOrderPayment".to_string())));
    assert_eq!(
        variants(&order, "PlaceOrderPayment"),
        pairs(&[
            ("Card", "(String)"),
            ("Credit", "(crate::domain::common::Money)"),
            ("Gift", "(PlaceOrderGift)"),
            ("Speed", "(PlaceOrderSpeed)"),
            ("DeferredUntil", "(::angzarr_embedded::Timestamp)"),
        ])
    );
}

/// A oneof's arm is converted through prost's own choice
/// (`<message module>::<Oneof>`), and an unset oneof is refused as
/// missing, as a message field is.
#[test]
fn test_a_oneof_converts_through_prosts_choice_and_is_required() {
    let source = source("shapes.order.v1");
    for expected in [
        "crate::wire::shapes::order::v1::place_order::Payment::Card(",
        "crate::wire::shapes::order::v1::place_order::Payment::Credit(",
        "WireError::Missing{message:\"shapes.order.v1.PlaceOrder\",field:\"payment\"",
    ] {
        assert!(
            flat(&source).contains(expected),
            "no `{expected}` in:\n{source}"
        );
    }
}

/// A nested message is named after its enclosing messages, as a nested
/// enum is, and converts to and from prost's type in the enclosing
/// message's module.
#[test]
fn test_a_nested_message_is_named_after_its_enclosing_messages() {
    let order = layer("shapes.order.v1");
    assert_eq!(
        fields(&order, "PlaceOrderGift"),
        pairs(&[("code", "String"), ("wrap", "PlaceOrderGiftWrap")])
    );
    assert!(item_struct(&order, "PlaceOrderGiftWrap").is_some());
    let impls = impls(&order);
    assert!(
        impls.contains(&(
            "From<PlaceOrderGiftWrap>".to_string(),
            "crate::wire::shapes::order::v1::place_order::gift::Wrap".to_string()
        )),
        "{impls:?}"
    );
    assert_eq!(
        variants(&order, "PlaceOrderSpeed"),
        pairs(&[("Slow", ""), ("Fast", "")])
    );
}

/// A choice taking the name of a type of its package is refused, naming
/// both: nothing is renamed silently.
#[test]
fn test_a_choice_taking_a_types_name_is_refused() {
    const FILE: &str = "bad/v1/oneof_name.proto";
    assert_eq!(
        markup_domain(
            &descriptors(&[FILE]),
            &[FILE],
            &[("bad.v1", "::bad_proto::v1")]
        ),
        Err(CodegenError::Proto(ProtoError::NameTaken {
            name: "PokeShape".to_string(),
            first: "bad.v1.PokeShape".to_string(),
            second: "bad.v1.Poke.shape".to_string(),
        }))
    );
}

// endregion oneofs and nested types

/// `Default` is derived for every domain type all of whose fields have
/// one; a required enum or a oneof has none, nor has a type holding one.
#[test]
fn test_default_is_derived_where_every_field_has_one() {
    let order = layer("shapes.order.v1");
    for defaulted in ["OrderPlaced", "OrderState", "PlaceOrderGift", "CustomerId"] {
        assert!(
            derives(&order, defaulted).contains("Default"),
            "{defaulted}: {}",
            derives(&order, defaulted)
        );
    }
    for plain in ["PlaceOrder", "PlaceOrderPayment", "PlaceOrderSpeed"] {
        assert!(
            !derives(&order, plain).contains("Default"),
            "{plain}: {}",
            derives(&order, plain)
        );
    }
    assert!(derives(&layer("shapes.common.v1"), "Money").contains("Default"));
}

// region state anchors

/// An anchor declaring `state` is a domain type of its layer (its
/// `Default` the empty state), a `DomainMessage` like an input or event,
/// and its component's interface is `<Anchor>Interface`.
#[test]
fn test_a_state_anchor_is_a_domain_type_beside_its_interface() {
    let order = layer("shapes.order.v1");
    assert_eq!(
        fields(&order, "OrderState"),
        pairs(&[
            ("placed_at", "Option<::angzarr_embedded::Timestamp>"),
            ("skus", "Vec<String>"),
            ("counts", "::std::collections::BTreeMap<String,u32>"),
            ("customer", "Option<CustomerId>"),
        ])
    );
    assert!(derives(&order, "OrderState").contains("Default"));
    assert!(impls(&order).contains(&(
        "::angzarr_embedded::markup::DomainMessage".to_string(),
        "OrderState".to_string()
    )));
    assert!(
        names(&order).contains(&"OrderStateInterface".to_string()),
        "{:?}",
        names(&order)
    );
}

/// A state anchor's adapter delegates to `<Anchor>Interface`, and its
/// scaffold implements that interface on a state of the anchor's domain
/// type.
#[test]
fn test_a_state_anchors_adapter_and_scaffold_name_its_interface() {
    let set = descriptors(SHAPES);
    let source = adapters(&set, SHAPES, &[]).unwrap();
    assert!(
        flat(&source).contains("crate::domain::OrderStateInterface::place_order"),
        "{source}"
    );
    let files = scaffold(&set, SHAPES).unwrap();
    let order = files
        .iter()
        .find(|f| f.path == "order_state_impl.rs")
        .unwrap_or_else(|| panic!("{:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>()));
    assert!(
        order
            .source
            .contains("impl OrderStateInterface for OrderStateImpl {"),
        "{}",
        order.source
    );
    assert!(
        order.source.contains("pub state: OrderState,"),
        "{}",
        order.source
    );
}

/// An anchor without `state` keeps the interface its name, as before.
#[test]
fn test_an_anchor_without_state_keeps_its_interface_name() {
    let generated = markup_domain(&descriptors(BUYIN), BUYIN, BUYIN_WIRE).unwrap();
    let file = syn::parse_file(&generated[0].source).unwrap();
    assert!(names(&file).contains(&"WalletState".to_string()));
    assert!(!names(&file).contains(&"WalletStateInterface".to_string()));
}

// endregion state anchors
