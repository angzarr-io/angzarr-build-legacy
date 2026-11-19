//! An aggregate written by hand over the generated domain layer, as an
//! application writes one: its state the anchor's domain type, snapshotted
//! as the anchor's wire type through the conversions the layer generates.

use angzarr_embedded::WireError;
use angzarr_embedded_macros::{aggregate, applies, handles_cmd};

use crate::domain::order::*;
use crate::wire::shapes::order::v1 as wire;

/// The orders domain, its state `shapes.order.v1.OrderState`.
#[derive(Clone, Debug, Default)]
pub struct Orders {
    pub state: OrderState,
}

#[aggregate(domain = "orders", snapshot = wire::OrderState)]
impl Orders {
    #[handles_cmd(PlaceOrder)]
    fn place(&self, cmd: PlaceOrder, _ctx: &Context) -> Result<OrderPlaced, Failed<Unrefusable>> {
        Ok(OrderPlaced {
            items: cmd.items,
            customer: cmd.customer,
            wait: cmd.wait,
            at: None,
        })
    }

    #[applies(OrderPlaced)]
    fn placed(&mut self, event: OrderPlaced) {
        for item in event.items {
            *self.state.counts.entry(item.sku.clone()).or_default() += 1;
            self.state.skus.push(item.sku);
        }
        self.state.customer = event.customer;
        self.state.placed_at = event.at;
    }
}

/// The snapshot: the state's wire type.
impl From<&Orders> for wire::OrderState {
    fn from(orders: &Orders) -> Self {
        orders.state.clone().into()
    }
}

impl Orders {
    /// Resumes from a snapshot: what the domain type cannot hold is
    /// refused, as an event's would be.
    pub fn restore(&mut self, snapshot: wire::OrderState) -> Result<(), WireError> {
        self.state = OrderState::try_from(snapshot)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use angzarr_embedded::State;
    use angzarr_embedded::v1::Uuid;

    use super::*;
    use crate::domain::catalog::Item;
    use crate::domain::common::Money;

    fn item(sku: &str) -> Item {
        Item {
            sku: sku.to_string(),
            price: Money {
                cents: 5,
                currency: "EUR".to_string(),
            },
            note: None,
        }
    }

    /// The aggregate after one order of two of `a` and one of `b`.
    fn placed() -> Orders {
        let mut orders = Orders::default();
        orders.placed(OrderPlaced {
            items: vec![item("a"), item("b"), item("a")],
            customer: Some(CustomerId(Uuid::from_bytes([7; 16]))),
            wait: prost_types::Duration::default(),
            at: Some(angzarr_embedded::Timestamp {
                seconds: 3,
                nanos: 0,
            }),
        });
        orders
    }

    #[test]
    fn the_state_is_snapshotted_as_the_anchors_wire_type() {
        assert!(<Orders as State>::SNAPSHOTS);
        assert_eq!(
            <Orders as State>::descriptor().type_name,
            "shapes.order.v1.OrderState"
        );
        let snapshot = placed().to_snapshot();
        assert_eq!(snapshot.skus, ["a", "b", "a"]);
        assert_eq!(snapshot.counts.get("a"), Some(&2));
        assert_eq!(
            snapshot.customer.as_ref().map(|c| c.value.clone()),
            Some(vec![7; 16])
        );
    }

    #[test]
    fn a_snapshot_restores_the_state_it_was_taken_of() {
        let orders = placed();
        let restored = Orders::from_snapshot(orders.to_snapshot()).expect("it restores");
        assert_eq!(restored.state, orders.state);
        let mut onto = placed();
        onto.state.skus.clear();
        State::restore(&mut onto, orders.to_snapshot()).expect("it restores");
        assert_eq!(onto.state, orders.state);
    }

    #[test]
    fn a_snapshot_the_domain_type_cannot_hold_is_refused() {
        let mut snapshot = placed().to_snapshot();
        snapshot.customer = Some(Uuid { value: vec![1, 2] });
        assert!(Orders::from_snapshot(snapshot).is_err());
    }
}
