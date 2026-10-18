#[derive(Debug, Default)]
pub struct CashShop {
    pub cash_points: u32,
    pub free_points: u32,
    pub tabs: Vec<Vec<(u32, u32)>>,
    pub cart: Vec<(u32, u32, u16, u32)>,
    pub free_to_spend: u32,
}

impl CashShop {
    pub fn new(cash_points: u32, free_points: u32) -> Self {
        Self { cash_points, free_points, tabs: vec![Vec::new(); 9], cart: Vec::new(), free_to_spend: 0 }
    }

    pub fn add_to_cart(&mut self, _item: u32, _amount: u32, _tab: u16, _price: u32) -> bool {
        false
    }

    pub fn set_amount(&mut self, _item: u32, _tab: u16, _amount: u32) -> bool {
        false
    }

    pub fn remove(&mut self, _item: u32, _tab: u16) -> bool {
        false
    }

    pub fn cart_total(&self) -> u32 {
        0
    }

    pub fn set_free_spend(&mut self, _amount: u32) {}

    pub fn buy_request(&self, _confirmed: bool) -> Option<(u32, Vec<(u32, u32, u16)>)> {
        None
    }

    pub fn clear_cart_on_success(&mut self, _result: u16, _cash: u32, _free: u32) {}
}

pub fn buy_result_message(_result: u16) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cart_accepts_amounts_one_through_ninety_nine() {
        let mut shop = CashShop::new(100, 50);
        assert_eq!(shop.tabs.len(), 9);
        assert!(!shop.add_to_cart(501, 0, 0, 10));
        assert!(!shop.add_to_cart(501, 100, 0, 10));
        assert!(shop.add_to_cart(501, 1, 0, 10));
        assert!(shop.add_to_cart(502, 99, 0, 20));
    }

    #[test]
    fn cart_updates_and_removes_an_item_in_a_tab() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 1, 3, 10));
        assert!(shop.add_to_cart(501, 7, 3, 10));
        assert_eq!(shop.cart, [(501, 7, 3, 10)]);
        assert!(shop.set_amount(501, 3, 99));
        assert_eq!(shop.cart[0].1, 99);
        assert!(!shop.set_amount(501, 3, 0));
        assert!(!shop.set_amount(501, 3, 100));
        assert!(shop.remove(501, 3));
        assert!(shop.cart.is_empty());
    }

    #[test]
    fn cart_totals_price_times_amount() {
        let mut shop = CashShop::new(100, 50);
        shop.cart = vec![(501, 3, 0, 10), (1201, 2, 4, 125)];
        assert_eq!(shop.cart_total(), 280);
    }

    #[test]
    fn free_spend_is_capped_by_points_and_cart_total() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 3, 0, 10));
        shop.set_free_spend(40);
        assert_eq!(shop.free_to_spend, 30);
        shop.cart[0].1 = 10;
        shop.set_free_spend(75);
        assert_eq!(shop.free_to_spend, 50);
    }

    #[test]
    fn buy_request_needs_a_cart_and_confirmation() {
        let mut shop = CashShop::new(100, 50);
        assert_eq!(shop.buy_request(true), None);
        shop.cart.push((501, 2, 3, 10));
        shop.free_to_spend = 15;
        assert_eq!(shop.buy_request(false), None);
        assert_eq!(shop.buy_request(true), Some((15, vec![(501, 2, 3)])));
    }

    #[test]
    fn successful_buy_clears_cart_and_updates_points() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 2, 3, 10));
        shop.free_to_spend = 15;
        shop.clear_cart_on_success(1, 90, 40);
        assert_eq!((shop.cash_points, shop.free_points, shop.cart.len()), (100, 50, 1));
        shop.clear_cart_on_success(0, 90, 40);
        assert_eq!((shop.cash_points, shop.free_points, shop.free_to_spend), (90, 40, 0));
        assert!(shop.cart.is_empty());
    }

    #[test]
    fn maps_buy_results_to_messages() {
        // ponytail: result messages follow the issue table until every server reason is observed.
        let messages = (0..=12).map(buy_result_message).collect::<Vec<_>>();
        assert_eq!(messages, [2530, 1228, 1254, 1230, 2536, 3568, 1229, 1940, 1814, 1944, 1945, 1946, 1947]);
    }
}
